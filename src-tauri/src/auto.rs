//! Máquina de estados del Notetaker automático (estilo Fathom), sin tocar el sistema.
//!
//! Cada ~3 s el detector le da la hora (en ms desde que arrancó), el modo elegido, si hay una
//! reunión grabándose y las apps de reunión que usan el micrófono ahora; la máquina contesta con
//! la acción a ejecutar (empezar, parar, avisar) o nada. Así toda la lógica de «cuándo» se prueba
//! con tiempos simulados.

use crate::models::MeetingAuto;

/// Una reunión automática se para sola cuando su app lleva tanto tiempo sin usar el micrófono
/// (cubre cortes, cambios de dispositivo y el típico «te he perdido, vuelvo a entrar»).
pub const END_GRACE_MS: u64 = 20_000;
/// Tras parar a mano una reunión, hasta que la app no suelta el micro durante esto no se vuelve a
/// disparar (evita reabrirla en la misma llamada).
pub const RELEASE_MS: u64 = 6_000;
/// En modo «ask»: lecturas seguidas sin reunión para dar por terminada la detectada y rearmar el aviso.
const ASK_MISSES: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoAction {
    /// Empezar a tomar notas automáticamente de esta app.
    Start(String),
    /// Parar la reunión automática en curso (la app colgó).
    Stop,
    /// Modo «ask»: avisar de que hay una reunión.
    Notify(String),
}

#[derive(Debug, Default)]
pub struct AutoMachine {
    /// App que disparó la reunión automática en curso.
    auto_app: Option<String>,
    /// Última vez que esa app usaba el micrófono.
    last_seen_ms: u64,
    /// Hay una reunión manual grabándose, y la app de reunión que sonaba cuando empezó.
    manual_recording: bool,
    manual_app: Option<String>,
    /// App que no debe volver a disparar hasta que suelte el micrófono.
    blocked: Option<String>,
    release_since_ms: Option<u64>,
    /// Modo «ask»: app ya avisada.
    notified: Option<String>,
    misses: u32,
}

impl AutoMachine {
    pub fn new() -> Self {
        Self::default()
    }

    /// ¿Hay una reunión automática en curso según la máquina?
    #[cfg(test)]
    pub fn has_auto_meeting(&self) -> bool {
        self.auto_app.is_some()
    }

    /// Un paso. `recording`: hay una reunión grabándose (la que sea). `in_use`: apps de reunión con
    /// el micrófono abierto ahora, la más probable primero.
    pub fn step(&mut self, now_ms: u64, mode: MeetingAuto, recording: bool, in_use: &[String]) -> Option<AutoAction> {
        if mode == MeetingAuto::Off {
            *self = Self::default();
            return None;
        }
        let present = |app: &str| in_use.iter().any(|a| a == app);

        if recording {
            self.blocked = None;
            self.release_since_ms = None;
            if let Some(app) = self.auto_app.clone() {
                if present(&app) {
                    self.last_seen_ms = now_ms;
                } else if now_ms.saturating_sub(self.last_seen_ms) >= END_GRACE_MS {
                    // Se para sola; no se bloquea: si vuelven a llamar, es otra reunión.
                    self.auto_app = None;
                    return Some(AutoAction::Stop);
                }
            } else if !self.manual_recording {
                // Reunión iniciada a mano: nunca se para sola.
                self.manual_recording = true;
                self.manual_app = in_use.first().cloned();
            } else if self.manual_app.is_none() {
                self.manual_app = in_use.first().cloned();
            }
            return None;
        }

        // No se está grabando. Si veníamos de una reunión que no cerró la propia máquina, la
        // cerró el usuario (o el arranque falló): esa llamada no se vuelve a disparar.
        if let Some(app) = self.auto_app.take() {
            self.blocked = Some(app);
            self.release_since_ms = None;
        }
        if std::mem::take(&mut self.manual_recording) {
            if let Some(app) = self.manual_app.take() {
                self.blocked = Some(app);
                self.release_since_ms = None;
            }
        }
        if let Some(blocked) = self.blocked.clone() {
            if present(&blocked) {
                self.release_since_ms = None;
                return None;
            }
            let since = *self.release_since_ms.get_or_insert(now_ms);
            if now_ms.saturating_sub(since) < RELEASE_MS {
                return None;
            }
            self.blocked = None;
            self.release_since_ms = None;
        }

        match in_use.first() {
            Some(app) => {
                self.misses = 0;
                match mode {
                    MeetingAuto::Auto => {
                        self.auto_app = Some(app.clone());
                        self.last_seen_ms = now_ms;
                        Some(AutoAction::Start(app.clone()))
                    }
                    MeetingAuto::Ask if self.notified.as_deref() != Some(app.as_str()) => {
                        self.notified = Some(app.clone());
                        Some(AutoAction::Notify(app.clone()))
                    }
                    _ => None,
                }
            }
            None => {
                self.misses += 1;
                if self.misses >= ASK_MISSES {
                    self.notified = None;
                }
                None
            }
        }
    }
}

/// Título de una reunión automática: «Reunión de Microsoft Teams · 10:32».
pub fn auto_title(app: &str, hhmm: &str) -> String {
    format!("Reunión de {app} · {hhmm}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use AutoAction::*;

    const A: MeetingAuto = MeetingAuto::Auto;

    fn teams() -> Vec<String> {
        vec!["Microsoft Teams".to_string()]
    }
    fn none() -> Vec<String> {
        Vec::new()
    }

    /// Avanza la máquina cada 3 s como el detector real; devuelve (segundo, acción) de cada acción.
    struct Sim {
        m: AutoMachine,
        t: u64,
        recording: bool,
    }
    impl Sim {
        fn new() -> Self {
            Self { m: AutoMachine::new(), t: 0, recording: false }
        }
        /// Un tick con `in_use`; aplica Start/Stop como lo haría `meeting.rs`.
        fn tick(&mut self, mode: MeetingAuto, in_use: &[String]) -> Option<AutoAction> {
            self.t += 3_000;
            let a = self.m.step(self.t, mode, self.recording, in_use);
            match &a {
                Some(Start(_)) => self.recording = true,
                Some(Stop) => self.recording = false,
                _ => {}
            }
            a
        }
        fn run(&mut self, mode: MeetingAuto, secs: u64, in_use: &[String]) -> Vec<(u64, AutoAction)> {
            let mut out = Vec::new();
            for _ in 0..secs / 3 {
                if let Some(a) = self.tick(mode, in_use) {
                    out.push((self.t / 1000, a));
                }
            }
            out
        }
    }

    #[test]
    fn arranca_sola_al_detectar_y_una_vez() {
        let mut s = Sim::new();
        assert!(s.run(A, 9, &none()).is_empty());
        let got = s.run(A, 60, &teams());
        assert_eq!(got, [(12, Start("Microsoft Teams".into()))]);
        assert!(s.m.has_auto_meeting());
    }

    #[test]
    fn se_para_sola_a_los_20_s_de_colgar() {
        let mut s = Sim::new();
        s.run(A, 30, &teams()); // arranca en t=3 y sigue en uso hasta t=30
        let got = s.run(A, 60, &none());
        // Última vez en uso: t=30. Stop en cuanto pasan 20 s: t=51 (30+21 en ticks de 3 s).
        assert_eq!(got, [(51, Stop)]);
        assert!(!s.m.has_auto_meeting());
    }

    #[test]
    fn un_corte_corto_no_para_la_reunion() {
        let mut s = Sim::new();
        s.run(A, 30, &teams());
        // 15 s sin micro (cambio de dispositivo) y vuelve: ni Stop ni nuevo Start.
        assert!(s.run(A, 15, &none()).is_empty());
        assert!(s.run(A, 60, &teams()).is_empty());
        assert!(s.recording);
        // Y la gracia se reinicia: otros 15 s fuera tampoco.
        assert!(s.run(A, 15, &none()).is_empty());
    }

    #[test]
    fn la_reunion_manual_nunca_se_para_sola() {
        let mut s = Sim::new();
        s.recording = true; // el usuario la empezó a mano
        assert!(s.run(A, 300, &teams()).is_empty());
        assert!(s.run(A, 300, &none()).is_empty());
        assert!(s.recording);
    }

    #[test]
    fn parada_manual_de_la_automatica_no_la_reabre_en_la_misma_llamada() {
        let mut s = Sim::new();
        s.run(A, 15, &teams()); // arranca
        s.recording = false; // el usuario pulsa ■
        // La llamada sigue: no se vuelve a disparar nunca.
        assert!(s.run(A, 120, &teams()).is_empty());
        // Cuelga: 6 s sin micro rearman la detección...
        assert!(s.run(A, 9, &none()).is_empty());
        // ...y una llamada nueva sí dispara.
        assert_eq!(s.run(A, 6, &teams()).len(), 1);
    }

    #[test]
    fn un_parpadeo_del_micro_tras_parar_a_mano_no_rearma() {
        let mut s = Sim::new();
        s.run(A, 15, &teams());
        s.recording = false;
        s.run(A, 3, &teams());
        s.run(A, 3, &none()); // 3 s sin micro
        assert!(s.run(A, 6, &teams()).is_empty());
    }

    #[test]
    fn parar_a_mano_una_reunion_manual_tampoco_dispara_la_automatica() {
        let mut s = Sim::new();
        s.recording = true;
        s.run(A, 30, &teams());
        s.recording = false; // la para el usuario con la llamada en marcha
        assert!(s.run(A, 120, &teams()).is_empty());
    }

    #[test]
    fn arranque_fallido_no_se_reintenta_en_bucle() {
        let mut s = Sim::new();
        // Start sin que `recording` llegue a ser true (falló abrir el audio).
        s.t += 3_000;
        assert_eq!(s.m.step(s.t, A, false, &teams()), Some(Start("Microsoft Teams".into())));
        s.recording = false;
        assert!(s.run(A, 120, &teams()).is_empty());
    }

    #[test]
    fn modo_ask_avisa_una_vez_por_reunion() {
        let mut s = Sim::new();
        let got = s.run(MeetingAuto::Ask, 60, &teams());
        assert_eq!(got, [(3, Notify("Microsoft Teams".into()))]);
        // Cuelga (6 s) y otra reunión: nuevo aviso.
        s.run(MeetingAuto::Ask, 9, &none());
        assert_eq!(s.run(MeetingAuto::Ask, 6, &teams()).len(), 1);
        assert!(!s.m.has_auto_meeting());
    }

    #[test]
    fn modo_off_no_hace_nada_y_olvida() {
        let mut s = Sim::new();
        assert!(s.run(MeetingAuto::Off, 60, &teams()).is_empty());
        s.run(A, 15, &teams());
        assert!(s.recording);
        s.run(MeetingAuto::Off, 3, &teams());
        assert!(!s.m.has_auto_meeting());
    }

    #[test]
    fn la_reunion_automatica_sigue_a_su_app_aunque_haya_otra() {
        let mut s = Sim::new();
        s.run(A, 6, &teams());
        // Aparece además Zoom delante en la lista, pero Teams sigue en uso: no se para.
        let both = vec!["Zoom".to_string(), "Microsoft Teams".to_string()];
        assert!(s.run(A, 60, &both).is_empty());
        assert!(s.recording);
    }

    #[test]
    fn titulo_automatico() {
        assert_eq!(auto_title("Microsoft Teams", "10:32"), "Reunión de Microsoft Teams · 10:32");
    }
}
