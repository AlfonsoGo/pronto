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

/// Qué se está grabando cuando el detector mira.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recording {
    /// Nada.
    Idle,
    /// La reunión la empezó el detector (`AutoAction::Start`).
    Auto,
    /// La empezó el usuario (atajo, botón o el aviso del modo «ask»): nunca se para sola.
    Manual,
}

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

    /// Un paso. `recording`: qué se está grabando (nada, la reunión automática o una manual).
    /// `in_use`: apps de reunión con el micrófono abierto ahora, la más probable primero.
    pub fn step(&mut self, now_ms: u64, mode: MeetingAuto, recording: Recording, in_use: &[String]) -> Option<AutoAction> {
        self.step_browsers(now_ms, mode, recording, in_use, &[])
    }

    /// Como `step`, con `browsers`: los navegadores (`"Chrome"`, `"Edge"`…) que tienen el micro en
    /// uso ahora mismo, sea cual sea su pestaña. Windows solo da el título de la pestaña ACTIVA, así
    /// que al mirar el correo en otra pestaña durante una llamada la etiqueta («Microsoft Teams
    /// (Edge)») desaparece aunque la llamada siga. El título solo vale para EMPEZAR; una reunión de
    /// navegador en marcha se sigue por el navegador (rompedor 23-oct, reuniones MEDIA 2).
    pub fn step_browsers(&mut self, now_ms: u64, mode: MeetingAuto, recording: Recording, in_use: &[String], browsers: &[String]) -> Option<AutoAction> {
        // Con «No» no se empieza ni se avisa nada, pero una automática en marcha prometió «Se para
        // solo al colgar»: se sigue hasta que cuelgue (rompedor 23-oct, reuniones BAJA 8).
        if mode == MeetingAuto::Off && recording != Recording::Auto {
            *self = Self::default();
            return None;
        }
        let present = |app: &str| in_use.iter().any(|a| a == app) || crate::detect::browser_of(app).is_some_and(|b| browsers.iter().any(|x| x == b));

        if recording != Recording::Idle {
            self.blocked = None;
            self.release_since_ms = None;
            match self.auto_app.clone() {
                Some(app) if recording == Recording::Auto => {
                    if present(&app) {
                        self.last_seen_ms = now_ms;
                    } else if now_ms.saturating_sub(self.last_seen_ms) >= END_GRACE_MS {
                        // Se para sola; no se bloquea: si vuelven a llamar, es otra reunión.
                        self.auto_app = None;
                        return Some(AutoAction::Stop);
                    }
                }
                // Automática que la máquina no recuerda (se reinició con el modo «No»): se adopta con
                // la app que suena ahora; si no suena nada, se para a los 20 s como cualquier otra.
                None if recording == Recording::Auto => {
                    self.auto_app = Some(in_use.first().cloned().unwrap_or_default());
                    self.last_seen_ms = now_ms;
                }
                _ => {
                    // Reunión iniciada a mano: nunca se para sola. Si la máquina aún creía grabar
                    // la automática, el usuario la paró y empezó esta antes del siguiente vistazo
                    // (rompedor 22-oct-c, BAJA 4): se olvida, y su app queda como la de la manual.
                    let previous_auto = self.auto_app.take();
                    if !self.manual_recording {
                        self.manual_recording = true;
                        self.manual_app = previous_auto.or_else(|| in_use.first().cloned());
                    } else if self.manual_app.is_none() {
                        self.manual_app = in_use.first().cloned();
                    }
                }
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

// ------------------------------------------------------------ la primera vez se pregunta

/// Cuánto tiempo vale un «sí» a la pregunta de la primera vez: lo que tarda el usuario en ver el
/// aviso y pulsar «Grabar» (la tarjeta se queda unos segundos; la píldora de la Flow bar sigue
/// ofreciendo empezar mientras dure la llamada). Pasado esto, empezar una reunión a mano ya no cuenta
/// como haber contestado.
pub const FIRST_ASK_WINDOW_MS: u64 = 120_000;

/// Decisión de Txonso, 23-oct-2026: la primera vez que la detección automática fuera a grabar
/// (`Start`), se pregunta con el mismo aviso del modo «ask» (`Notify`: «¿Grabar esta reunión en
/// Meets?»). Hasta que no hay un sí (`confirmed`), `Start` se convierte en `Notify`; lo demás pasa tal
/// cual. La máquina ya cuenta esa llamada como suya (como un arranque que falló): si el usuario dice
/// que no, no se graba y no se insiste hasta que la llamada acabe; la siguiente vuelve a preguntar.
pub fn first_time_gate(action: Option<AutoAction>, confirmed: bool) -> Option<AutoAction> {
    match action {
        Some(AutoAction::Start(app)) if !confirmed => Some(AutoAction::Notify(app)),
        other => other,
    }
}

/// Quien ya tiene reuniones grabadas viene de una versión que grababa sin preguntar y sigue así: no
/// se le pregunta de repente (decisión de Txonso, 23-oct-2026).
pub fn confirmed_with_history(confirmed: bool, meetings: usize) -> bool {
    confirmed || meetings > 0
}

/// La pregunta de la primera vez está en pantalla (o lo estuvo hace poco). Si el usuario empieza la
/// reunión mientras tanto, es el «sí»; si no, no hay nada que recordar y se vuelve a preguntar en la
/// próxima llamada. El tiempo (ms) lo pone quien llama, para probarlo sin reloj.
#[derive(Debug, Default)]
pub struct FirstAsk {
    asked_at_ms: Option<u64>,
}

impl FirstAsk {
    /// Se acaba de enseñar la pregunta.
    pub fn asked(&mut self, now_ms: u64) {
        self.asked_at_ms = Some(now_ms);
    }

    /// El usuario empezó una reunión a mano: ¿era la respuesta a la pregunta? Vale una sola vez.
    pub fn accepted(&mut self, now_ms: u64) -> bool {
        self.asked_at_ms.take().is_some_and(|t| now_ms.saturating_sub(t) <= FIRST_ASK_WINDOW_MS)
    }
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
        recording: Recording,
    }
    impl Sim {
        fn new() -> Self {
            Self { m: AutoMachine::new(), t: 0, recording: Recording::Idle }
        }
        /// Un tick con `in_use`; aplica Start/Stop como lo haría `meeting.rs`.
        fn tick(&mut self, mode: MeetingAuto, in_use: &[String]) -> Option<AutoAction> {
            self.t += 3_000;
            let a = self.m.step(self.t, mode, self.recording, in_use);
            match &a {
                Some(Start(_)) => self.recording = Recording::Auto,
                Some(Stop) => self.recording = Recording::Idle,
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
        assert_ne!(s.recording, Recording::Idle);
        // Y la gracia se reinicia: otros 15 s fuera tampoco.
        assert!(s.run(A, 15, &none()).is_empty());
    }

    #[test]
    fn la_reunion_manual_nunca_se_para_sola() {
        let mut s = Sim::new();
        s.recording = Recording::Manual; // el usuario la empezó a mano
        assert!(s.run(A, 300, &teams()).is_empty());
        assert!(s.run(A, 300, &none()).is_empty());
        assert_ne!(s.recording, Recording::Idle);
    }

    #[test]
    fn parada_manual_de_la_automatica_no_la_reabre_en_la_misma_llamada() {
        let mut s = Sim::new();
        s.run(A, 15, &teams()); // arranca
        s.recording = Recording::Idle; // el usuario pulsa ■
        // La llamada sigue: no se vuelve a disparar nunca.
        assert!(s.run(A, 120, &teams()).is_empty());
        // Cuelga: 6 s sin micro rearman la detección...
        assert!(s.run(A, 9, &none()).is_empty());
        // ...y una llamada nueva sí dispara.
        assert_eq!(s.run(A, 6, &teams()).len(), 1);
    }

    /// Rompedor 22-oct-c, BAJA 4: el usuario para la automática y empieza otra a mano antes del
    /// siguiente vistazo (3 s). La manual nunca se para sola, y al pararla esa llamada no redispara.
    #[test]
    fn manual_empezada_justo_tras_parar_la_automatica_no_se_para_sola() {
        let mut s = Sim::new();
        s.run(A, 6, &teams()); // automática en marcha
        s.recording = Recording::Manual; // ■ y ● entre dos vistazos
        assert!(s.run(A, 60, &teams()).is_empty());
        assert!(s.run(A, 300, &none()).is_empty(), "la manual no se para al colgar");
        assert!(!s.m.has_auto_meeting());
        s.run(A, 3, &teams());
        s.recording = Recording::Idle; // la para el usuario con la llamada en marcha
        assert!(s.run(A, 60, &teams()).is_empty(), "esa llamada no vuelve a disparar");
    }

    #[test]
    fn un_parpadeo_del_micro_tras_parar_a_mano_no_rearma() {
        let mut s = Sim::new();
        s.run(A, 15, &teams());
        s.recording = Recording::Idle;
        s.run(A, 3, &teams());
        s.run(A, 3, &none()); // 3 s sin micro
        assert!(s.run(A, 6, &teams()).is_empty());
    }

    #[test]
    fn parar_a_mano_una_reunion_manual_tampoco_dispara_la_automatica() {
        let mut s = Sim::new();
        s.recording = Recording::Manual;
        s.run(A, 30, &teams());
        s.recording = Recording::Idle; // la para el usuario con la llamada en marcha
        assert!(s.run(A, 120, &teams()).is_empty());
    }

    #[test]
    fn arranque_fallido_no_se_reintenta_en_bucle() {
        let mut s = Sim::new();
        // Start sin que `recording` llegue a ser true (falló abrir el audio).
        s.t += 3_000;
        assert_eq!(s.m.step(s.t, A, Recording::Idle, &teams()), Some(Start("Microsoft Teams".into())));
        s.recording = Recording::Idle;
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
        // Parada a mano con la detección en «No»: se olvida todo.
        s.run(A, 15, &teams());
        s.recording = Recording::Idle;
        s.run(MeetingAuto::Off, 3, &teams());
        assert!(!s.m.has_auto_meeting());
        assert!(s.run(MeetingAuto::Off, 60, &teams()).is_empty());
    }

    /// Rompedor 23-oct, reuniones BAJA 8: la automática en marcha prometió «Se para solo al colgar»
    /// aunque la detección pase a «No» (con o sin volver a «Automático»).
    #[test]
    fn modo_off_no_suelta_una_automatica_en_marcha() {
        let mut s = Sim::new();
        s.run(A, 15, &teams());
        assert!(s.run(MeetingAuto::Off, 60, &teams()).is_empty(), "con «No» no se para mientras la llamada siga");
        assert!(s.m.has_auto_meeting());
        let got = s.run(MeetingAuto::Off, 60, &none());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, Stop);
        // Una máquina reiniciada (la automática ya no se recuerda) la adopta con la app que suena.
        let mut s = Sim::new();
        s.recording = Recording::Auto;
        assert!(s.run(A, 30, &teams()).is_empty());
        assert_eq!(s.run(A, 60, &none()).len(), 1);
    }

    /// Rompedor 23-oct, reuniones MEDIA 2: el título solo empieza la reunión de navegador; al cambiar
    /// de pestaña (el título deja de ser el de la llamada) se sigue por el navegador con el micro.
    #[test]
    fn cambiar_de_pestana_no_corta_la_reunion_de_navegador() {
        let mut s = Sim::new();
        let call = vec!["Microsoft Teams (Edge)".to_string()];
        let edge = vec!["Edge".to_string()];
        let mut step = |s: &mut Sim, secs: u64, in_use: &[String], browsers: &[String]| {
            let mut out = Vec::new();
            for _ in 0..secs / 3 {
                s.t += 3_000;
                let a = s.m.step_browsers(s.t, A, s.recording, in_use, browsers);
                match &a {
                    Some(Start(_)) => s.recording = Recording::Auto,
                    Some(Stop) => s.recording = Recording::Idle,
                    _ => {}
                }
                out.extend(a);
            }
            out
        };
        assert_eq!(step(&mut s, 30, &call, &edge).len(), 1);
        assert!(step(&mut s, 300, &none(), &edge).is_empty(), "otra pestaña, mismo micro del navegador: sigue");
        assert!(step(&mut s, 60, &call, &edge).is_empty(), "al volver no se parte en dos");
        // Cuelga: el navegador suelta el micro, aunque el título aún diga Teams.
        assert_eq!(step(&mut s, 30, &none(), &[]), [Stop]);
    }

    #[test]
    fn la_reunion_automatica_sigue_a_su_app_aunque_haya_otra() {
        let mut s = Sim::new();
        s.run(A, 6, &teams());
        // Aparece además Zoom delante en la lista, pero Teams sigue en uso: no se para.
        let both = vec!["Zoom".to_string(), "Microsoft Teams".to_string()];
        assert!(s.run(A, 60, &both).is_empty());
        assert_ne!(s.recording, Recording::Idle);
    }

    #[test]
    fn titulo_automatico() {
        assert_eq!(auto_title("Microsoft Teams", "10:32"), "Reunión de Microsoft Teams · 10:32");
    }

    // ---- la primera vez se pregunta (decisión de Txonso, 23-oct-2026)

    /// Detector simulado con la puerta de la primera vez: lo que haría `meeting::spawn_detector`.
    struct Primera {
        m: AutoMachine,
        ask: FirstAsk,
        t: u64,
        recording: Recording,
        confirmed: bool,
    }
    impl Primera {
        fn new(confirmed: bool) -> Self {
            Self { m: AutoMachine::new(), ask: FirstAsk::default(), t: 0, recording: Recording::Idle, confirmed }
        }
        fn run(&mut self, secs: u64, in_use: &[String]) -> Vec<(u64, AutoAction)> {
            let mut out = Vec::new();
            for _ in 0..secs / 3 {
                self.t += 3_000;
                let a = first_time_gate(self.m.step(self.t, A, self.recording, in_use), self.confirmed);
                match &a {
                    Some(Start(_)) => self.recording = Recording::Auto,
                    Some(Notify(_)) => self.ask.asked(self.t),
                    Some(Stop) => self.recording = Recording::Idle,
                    None => {}
                }
                out.extend(a.map(|a| (self.t / 1000, a)));
            }
            out
        }
        /// El usuario pulsa «Grabar» en el aviso: empieza una reunión a mano y, si era la respuesta, confirma.
        fn acepta(&mut self) {
            self.recording = Recording::Manual;
            if self.ask.accepted(self.t) {
                self.confirmed = true;
            }
        }
    }

    #[test]
    fn la_primera_vez_pregunta_en_vez_de_grabar() {
        let mut s = Primera::new(false);
        assert!(s.run(9, &none()).is_empty());
        let got = s.run(60, &teams());
        assert_eq!(got, [(12, Notify("Microsoft Teams".into()))], "una pregunta, ninguna grabación");
        assert_eq!(s.recording, Recording::Idle);
    }

    #[test]
    fn si_dice_que_si_se_graba_y_las_siguientes_van_solas() {
        let mut s = Primera::new(false);
        assert_eq!(s.run(15, &teams()).len(), 1);
        s.acepta();
        assert!(s.confirmed, "el sí se recuerda");
        // La reunión que aceptó es manual: no se para sola ni se vuelve a preguntar.
        assert!(s.run(120, &teams()).is_empty());
        assert!(s.run(120, &none()).is_empty());
        assert_eq!(s.recording, Recording::Manual);
        s.recording = Recording::Idle; // la para el usuario
        s.run(30, &none());
        // Llamada nueva: ya confirmado, empieza sola como siempre.
        assert_eq!(s.run(15, &teams()), [(s.t / 1000 - 15 + 3, Start("Microsoft Teams".into()))]);
    }

    #[test]
    fn si_dice_que_no_no_se_graba_ni_se_insiste_y_la_proxima_vuelve_a_preguntar() {
        let mut s = Primera::new(false);
        assert_eq!(s.run(15, &teams()).len(), 1);
        // No pulsa nada: la llamada sigue 10 minutos y no hay ni grabación ni segunda pregunta.
        assert!(s.run(600, &teams()).is_empty());
        assert_eq!(s.recording, Recording::Idle);
        assert!(!s.confirmed);
        // Cuelga y llega otra llamada: pregunta otra vez.
        s.run(15, &none());
        assert_eq!(s.run(15, &teams()).len(), 1);
        // Un «sí» tardío (mucho después de la pregunta) no cuenta como respuesta.
        s.t += FIRST_ASK_WINDOW_MS + 1;
        s.acepta();
        assert!(!s.confirmed);
    }

    #[test]
    fn confirmado_no_cambia_nada_y_ask_y_off_siguen_igual() {
        let mut s = Primera::new(true);
        assert_eq!(s.run(15, &teams()).iter().map(|(_, a)| a.clone()).collect::<Vec<_>>(), [Start("Microsoft Teams".into())]);
        // La puerta solo toca `Start`.
        for a in [Some(Stop), Some(Notify("Zoom".into())), None] {
            assert_eq!(first_time_gate(a.clone(), false), a);
            assert_eq!(first_time_gate(a.clone(), true), a);
        }
        assert_eq!(first_time_gate(Some(Start("Zoom".into())), false), Some(Notify("Zoom".into())));
        assert_eq!(first_time_gate(Some(Start("Zoom".into())), true), Some(Start("Zoom".into())));
    }

    #[test]
    fn quien_ya_tiene_reuniones_cuenta_como_confirmado() {
        assert!(!confirmed_with_history(false, 0));
        assert!(confirmed_with_history(false, 1));
        assert!(confirmed_with_history(false, 40));
        assert!(confirmed_with_history(true, 0));
    }

    #[test]
    fn el_si_solo_vale_una_vez_y_dentro_del_plazo() {
        let mut a = FirstAsk::default();
        assert!(!a.accepted(5_000), "sin pregunta no hay respuesta");
        a.asked(10_000);
        assert!(a.accepted(10_000 + FIRST_ASK_WINDOW_MS));
        assert!(!a.accepted(10_001), "ya se usó");
        a.asked(10_000);
        assert!(!a.accepted(10_000 + FIRST_ASK_WINDOW_MS + 1));
        assert!(!a.accepted(10_001), "una pregunta caducada se olvida");
    }
}
