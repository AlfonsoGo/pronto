//! Texto en vivo (BELTR-PRT-10), como Wispr Flow o SuperWhisper: mientras se graba un dictado, cada
//! `TICK` se transcribe lo dicho desde el último punto fijado y la Flow bar enseña esas palabras
//! provisionales (evento `dictation://live`). En cada pausa (`PAUSA` de silencio tras algo de voz) el
//! tramo hasta la pausa queda fijado y no se vuelve a transcribir.
//!
//! Es solo una vista previa: al soltar el atajo, `state.rs` transcribe el audio entero, como sin texto en
//! vivo, con los dos motores. A trozos el texto salía peor (12 WAV de Txonso, 19-nov: Parakeet perdía y
//! cambiaba palabras en 5 de 12 y ponía punto y mayúscula en cada corte; Whisper inventaba frases).
//!
//! Reglas que cumple:
//! - Cede el motor: cada tick usa `Engine::transcribe_live`, que nunca espera; si el motor está ocupado
//!   (un trozo de reunión a medias, un reintento del hub) o hay un dictado esperando turno, el tick se
//!   salta. Si un tick tarda más que `TICK`, el siguiente se salta (`proximo_tick`).
//! - Velocidad (regla 3): al soltar, la sesión para antes de cerrar la grabación y el dictado no espera
//!   al tick que estuviera a medias (con Parakeet van a la vez; `Engine::transcribe_live`).
//! - Todo en memoria y en este PC: el evento va solo a las ventanas de Pronto (regla 1).
//!
//! Lo puro (dónde cortar, cómo unir, cuándo toca el siguiente tick) se prueba sin micrófono ni modelo.

use crate::asr::Timed;
use crate::audio::{self, Tap};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cada cuánto se transcribe lo que se lleva dicho.
pub const TICK: Duration = Duration::from_millis(700);
/// Audio a 16 kHz, como el motor.
const RATE: usize = 16_000;
/// Tramo de análisis de la energía: 20 ms, como `segment.rs`.
const FRAME: usize = RATE / 50;
/// Silencio que fija lo dicho antes: 600 ms (30 tramos de 20 ms). Con 300 ms se fijaba también en las
/// comas: el motor, sin ver lo que sigue, cierra el tramo con punto («Si alguien no puede venir. Que me
/// avise hoy»). Medido con `nsn_vivo_soltar_hasta_texto`: en `dictado-15s.wav` las comas llevan pausas de 400 a
/// 560 ms y los puntos de 920 a 940 ms. Como `segment.rs`, que cierra a los 700 ms en vivo.
const PAUSA: usize = 30;
/// Voz mínima (en tramos) antes de una pausa para que el tramo merezca fijarse: 160 ms descarta
/// chasquidos y respiraciones.
const VOZ_MINIMA: usize = 8;
/// Un tramo no se fija si dura menos que esto: con trozos de una o dos palabras el motor pierde el
/// contexto (mayúsculas y puntos de más). Medido con `nsn_vivo_soltar_hasta_texto`.
const TRAMO_MINIMO: usize = RATE * 3 / 2;
/// Sin pausas, el tramo sin fijar no pasa de esto: se corta por su punto más silencioso. Así cada tick
/// transcribe pocos segundos aunque se hable sin respirar. Con Parakeet, el tick que va a medias al
/// soltar comparte la CPU con el dictado final (no se puede cortar): cuanto más corto, menos lo frena
/// (regla 3). Con 8 s, ticks de hasta 9 s y +80-430 ms al soltar.
const TRAMO_MAXIMO: usize = RATE * 4;
/// Donde se busca ese punto: los últimos 3 s del tramo, salvo el último medio segundo (la palabra que
/// se está diciendo ahora).
const BUSQUEDA_CORTE: usize = RATE * 3;
const MARGEN_CORTE: usize = RATE / 2;
/// Suelos de los umbrales de voz y de silencio (los de `segment.rs`).
const SUELO_VOZ: f32 = 0.004;
const SUELO_SILENCIO: f32 = 0.0024;
/// Ruido de fondo inicial y tramos de calibración rápida (300 ms).
const RUIDO_INICIAL: f32 = 0.002;
const CALIBRACION: usize = 15;
/// Por debajo de este pico no hay nada que transcribir (micrófono mudo: `state::SILENCE_PEAK`).
pub const PICO_MUDO: f32 = 0.0005;
/// Silencio que se deja detrás de la última voz al transcribir lo provisional (ayuda a la última
/// palabra; más silencio es donde Whisper inventa).
const COLA_VISTA: usize = RATE / 5;
/// Lo que sigue a una pausa recién fijada se transcribe en el mismo tick si ya dura esto.
const PROVISIONAL_MINIMO: usize = RATE * 3 / 10;

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

/// ¿No hay nada que transcribir (micro mudo o silencio digital)?
pub fn callado(samples: &[f32]) -> bool {
    samples.iter().all(|s| s.abs() < PICO_MUDO)
}

/// Decide dónde se fija el texto. Lleva la cuenta de lo fijado (en muestras a 16 kHz desde el principio
/// de la grabación) y del ruido de fondo, que aprende de los tramos sin voz como `segment.rs`.
#[derive(Debug, Clone)]
pub struct Fijador {
    fijado: usize,
    /// Tramos de silencio que fijan (`PAUSA`; las pruebas lo cambian para medir).
    pausa: usize,
    ruido: f32,
    /// Hasta dónde (muestras absolutas) ya se midió el ruido: cada tramo cuenta una vez.
    medido: usize,
}

impl Default for Fijador {
    fn default() -> Self {
        Self { fijado: 0, pausa: PAUSA, ruido: RUIDO_INICIAL, medido: 0 }
    }
}

impl Fijador {
    /// Un fijador que corta con pausas de `pausa` tramos de 20 ms (`SEG_PAUSA` para los segmentos).
    pub fn con_pausa(pausa: usize) -> Self {
        Self { pausa, ..Self::default() }
    }

    /// Fin de lo fijado (muestras a 16 kHz desde el principio).
    pub fn fijado(&self) -> usize {
        self.fijado
    }

    /// Lo fijado avanza `n` muestras.
    pub fn fijar(&mut self, n: usize) {
        self.fijado += n;
    }

    fn umbral_voz(&self) -> f32 {
        (self.ruido * 3.5).max(SUELO_VOZ)
    }

    fn umbral_silencio(&self) -> f32 {
        (self.ruido * 2.0).max(SUELO_SILENCIO)
    }

    /// Fin del último tramo con voz de `cola`, si tiene voz de verdad (al menos `VOZ_MINIMA` tramos por
    /// encima del umbral de voz); `None` si solo hay silencio, ruido o un chasquido.
    pub fn voz_hasta(&self, cola: &[f32]) -> Option<usize> {
        let voz = self.umbral_voz();
        let con_voz: Vec<usize> = cola.chunks_exact(FRAME).enumerate().filter(|(_, f)| rms(f) > voz).map(|(i, _)| i).collect();
        (con_voz.len() >= VOZ_MINIMA).then(|| (con_voz[con_voz.len() - 1] + 1) * FRAME)
    }

    /// `cola` es el audio desde lo fijado hasta ahora. Devuelve cuántas muestras de su principio fijar,
    /// o `None` si aún no toca: en la última pausa de al menos `PAUSA` que siga a voz y deje un tramo de
    /// `TRAMO_MINIMO` (se corta en su mitad), o, si la cola pasa de `TRAMO_MAXIMO` sin pausas, en su
    /// tramo más silencioso de la ventana de búsqueda (`Corte::pausa` = false).
    pub fn corte(&mut self, cola: &[f32]) -> Option<Corte> {
        let niveles = self.aprender(cola);
        self.corte_con(cola, &niveles)
    }

    /// Niveles de `cola` (un rms por tramo de 20 ms), aprendiendo de paso el ruido de fondo.
    fn aprender(&mut self, cola: &[f32]) -> Vec<f32> {
        let niveles: Vec<f32> = cola.chunks_exact(FRAME).map(rms).collect();
        // Ruido de fondo: solo con los tramos que no se habían medido y no son voz.
        let primero = self.medido.saturating_sub(self.fijado).div_ceil(FRAME);
        for (i, &l) in niveles.iter().enumerate().skip(primero) {
            let abs = (self.fijado + i * FRAME) / FRAME;
            if abs < CALIBRACION && l <= 0.03 {
                self.ruido = (0.7 * self.ruido + 0.3 * l).clamp(0.0003, 0.05);
            } else if l <= self.umbral_voz() {
                self.ruido = (0.97 * self.ruido + 0.03 * l).clamp(0.0003, 0.05);
            }
        }
        self.medido = self.medido.max(self.fijado + niveles.len() * FRAME);
        niveles
    }

    /// Corte de un segmento confirmado (BELTR-PRT-16): con `minimo` muestras o más desde lo fijado, la
    /// primera pausa de al menos `PAUSA` tras voz cuya mitad cae a partir de `minimo` (se corta en su
    /// mitad: ~300 ms de silencio a cada lado, el motor no se come la última palabra). Sin pausa, al
    /// llegar a `tope` se corta en el tramo más silencioso de sus últimos 6 s, como `asr::split_long`.
    pub fn corte_segmento(&mut self, cola: &[f32], minimo: usize, tope: usize) -> Option<usize> {
        if cola.len() < minimo {
            return None;
        }
        let niveles = self.aprender(cola);
        let (voz, silencio) = (self.umbral_voz(), self.umbral_silencio());
        let (mut racha, mut con_voz) = (0usize, 0usize);
        for (i, &l) in niveles.iter().enumerate() {
            if l <= silencio {
                racha += 1;
            } else {
                racha = 0;
                if l > voz {
                    con_voz += 1;
                }
            }
            if racha >= self.pausa && con_voz >= VOZ_MINIMA {
                let mitad = (i + 1 - racha) + racha / 2;
                if mitad * FRAME >= minimo {
                    return Some(mitad * FRAME);
                }
            }
        }
        if cola.len() < tope {
            return None;
        }
        let hasta = tope / FRAME;
        let desde = hasta.saturating_sub(RATE * 6 / FRAME);
        let mut mejor = (hasta, f32::MAX);
        for (i, &l) in niveles.iter().enumerate().take(hasta).skip(desde) {
            if l <= mejor.1 {
                mejor = (i + 1, l);
            }
        }
        Some(mejor.0 * FRAME)
    }

    /// Fin de la voz si `cola` acaba en un silencio de al menos `pausa` tramos tras voz de verdad:
    /// (muestras hasta el final de la voz, umbral de voz). Para anticipar el texto al dejar de hablar.
    pub fn fin_de_voz(&mut self, cola: &[f32], pausa: usize) -> Option<(usize, f32)> {
        let niveles = self.aprender(cola);
        let (voz, silencio) = (self.umbral_voz(), self.umbral_silencio());
        let ultima = niveles.iter().rposition(|&l| l > voz)?;
        let con_voz = niveles.iter().filter(|&&l| l > voz).count();
        let cola_silencio = niveles.len() - ultima - 1;
        (con_voz >= VOZ_MINIMA && cola_silencio >= pausa && niveles[ultima + 1..].iter().all(|&l| l <= silencio)).then_some(((ultima + 1) * FRAME, voz))
    }

    fn corte_con(&mut self, cola: &[f32], niveles: &[f32]) -> Option<Corte> {
        let (voz, silencio) = (self.umbral_voz(), self.umbral_silencio());
        let mut con_voz = 0usize;
        let mut racha = 0usize;
        let mut corte = None;
        for (i, &l) in niveles.iter().enumerate() {
            if l <= silencio {
                racha += 1;
            } else {
                racha = 0;
                if l > voz {
                    con_voz += 1;
                }
            }
            // La pausa (aún abierta o ya cerrada) se corta por su mitad: cada lado se lleva algo de silencio.
            if racha >= self.pausa && con_voz >= VOZ_MINIMA {
                let mitad = (i + 1 - racha) + racha / 2;
                if mitad * FRAME >= TRAMO_MINIMO {
                    corte = Some(mitad * FRAME);
                }
            }
        }
        if let Some(n) = corte {
            return Some(Corte { n, pausa: true });
        }
        if cola.len() <= TRAMO_MAXIMO {
            return None;
        }
        // Sin pausas: el tramo más silencioso (en empate, el más tardío) de la ventana de búsqueda.
        let hasta = (cola.len() - MARGEN_CORTE) / FRAME;
        let desde = hasta.saturating_sub(BUSQUEDA_CORTE / FRAME).max(hasta / 2);
        let mut mejor = (hasta, f32::MAX);
        for (i, &l) in niveles.iter().enumerate().take(hasta).skip(desde) {
            if l <= mejor.1 {
                mejor = (i + 1, l);
            }
        }
        Some(Corte { n: mejor.0 * FRAME, pausa: false })
    }
}

/// Dónde fijar: `n` muestras desde lo fijado, y si es en una pausa de verdad o un corte forzado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Corte {
    pub n: usize,
    pub pausa: bool,
}

/// Junta dos trozos de texto con un espacio (sin espacios dobles ni sobrantes).
fn unir(a: &str, b: &str) -> String {
    match (a.trim(), b.trim()) {
        ("", b) => b.to_string(),
        (a, "") => a.to_string(),
        (a, b) => format!("{a} {b}"),
    }
}

/// Lo que enseña la Flow bar: el texto fijado y, detrás, las palabras provisionales (en gris).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vista {
    pub fixed: String,
    pub partial: String,
}

impl Vista {
    /// La vista con el diccionario aplicado (BELTR-PRT-11): mientras se habla ya salen bien escritas
    /// las palabras guardadas («Txonso», no «Chonso»), y los correos, teléfonos y horas como se pegarán
    /// (BELTR-PRT-15). Solo texto, sin modelo: no cuesta tiempo.
    pub fn con_diccionario(&self, words: &[crate::models::Word]) -> Vista {
        let f = |t: &str| {
            let t = crate::text::format_spoken(t);
            if words.is_empty() { t } else { crate::text::apply_dictionary(&t, words) }
        };
        Vista { fixed: f(&self.fixed), partial: f(&self.partial) }
    }
}

/// Cuándo toca el próximo tick: el siguiente múltiplo de `tick` desde `t0` que aún no ha pasado. Si el
/// anterior tardó más que `tick` (PC lento), el que tocaba mientras tanto se salta en vez de amontonarse.
pub fn proximo_tick(t0: Instant, ahora: Instant, tick: Duration) -> Instant {
    let pasados = ahora.saturating_duration_since(t0).as_nanos() / tick.as_nanos().max(1);
    t0 + tick * (pasados as u32 + 1)
}

/// Transcribe un trozo; `None` = el motor está ocupado y este tick se salta.
pub type Decodificar = Box<dyn FnMut(&[f32]) -> Option<Result<Timed, String>> + Send>;

// ---------------------------------------------------------------- segmentos confirmados (BELTR-PRT-16)
//
// Mientras se habla, el audio largo se va confirmando por segmentos: cuando lo pendiente pasa de
// `SEG_MINIMO` y llega una pausa real, ese trozo se transcribe una vez, entero, con el mismo camino que
// el dictado (`state::transcribe_heard`) y se pule en segundo plano (`polish::Pulidor`). Al soltar solo
// queda transcribir y pulir la cola. Es lo que ya hacía el «audio entero» (Parakeet lo parte en trozos de
// 60 s, `asr::split_long`), pero antes y en pausas de verdad. Un dictado de menos de `SEG_MINIMO` no se
// confirma nunca: sale exactamente como antes. Decidido con Fable el 20-nov-2026
// (specs/features/pulido-en-directo.md).

/// Parakeet: segmentos de 20 s como mínimo; sin pausa, se corta a los 40 s.
pub const SEG_MINIMO: usize = RATE * 20;
pub const SEG_TOPE: usize = RATE * 40;
/// Whisper trabaja en ventanas de 30 s (`whisper.rs` parte a 28 s): segmentos de 15 a 28 s.
pub const SEG_MINIMO_WHISPER: usize = RATE * 15;
pub const SEG_TOPE_WHISPER: usize = RATE * 28;

/// Un segmento confirmado: dónde está (muestras a 16 kHz desde el principio) y lo que oyó el motor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmento {
    pub inicio: usize,
    pub fin: usize,
    pub crudo: String,
}

/// Texto anticipado (BELTR-PRT-16): al dejar de hablar (`FIN_PAUSA`), lo pendiente desde el último
/// segmento se transcribe entero en segundo plano. Si al soltar no hubo voz después, ese es el texto final:
/// el mismo audio por el mismo camino, sin el silencio del final.
#[derive(Debug, Clone, PartialEq)]
pub struct Anticipado {
    pub inicio: usize,
    pub hasta: usize,
    pub crudo: String,
    /// Umbral de voz con el ruido de fondo de ese momento (para ver si se habló después).
    umbral: f32,
}

#[cfg(test)]
impl Anticipado {
    /// Para las pruebas de la regla 5 en `state.rs` (soltar mientras se anticipa, BELTR-PRT-30).
    pub(crate) fn tests(inicio: usize, hasta: usize, crudo: &str, umbral: f32) -> Self {
        Self { inicio, hasta, crudo: crudo.into(), umbral }
    }
}

/// Silencio que dice «ha dejado de hablar» para anticipar el texto (15 tramos = 300 ms).
pub const FIN_PAUSA: usize = 15;

#[derive(Debug, Default)]
struct Estado {
    segmentos: Vec<Segmento>,
    /// Confirmando un segmento (tiene el motor): al soltar se espera.
    ocupado: bool,
    anticipado: Option<Anticipado>,
    /// Anticipando (inicio, hasta, umbral): al soltar solo se espera si no se habló después.
    anticipando: Option<(usize, usize, f32)>,
}

/// Lo confirmado de un dictado, compartido entre el hilo de vivo y el que transcribe al soltar.
#[derive(Debug, Default)]
pub struct Confirmados {
    estado: std::sync::Mutex<Estado>,
    hecho: std::sync::Condvar,
}

/// Voz que invalida el texto anticipado: 4 tramos (80 ms), la mitad de `VOZ_MINIMA`, para que un «sí» o
/// un «vale» dichos tras la pausa no se queden fuera (revisión del 20-nov-2026).
const VOZ_TRAS_ANTICIPO: usize = 4;

/// ¿Hay voz en `audio` (al menos `VOZ_TRAS_ANTICIPO` tramos por encima de `umbral`)?
fn hay_voz(audio: &[f32], umbral: f32) -> bool {
    audio.chunks_exact(FRAME).filter(|f| rms(f) > umbral).count() >= VOZ_TRAS_ANTICIPO
}

impl Confirmados {
    fn lock(&self) -> std::sync::MutexGuard<'_, Estado> {
        self.estado.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub(crate) fn empezar(&self) {
        self.lock().ocupado = true;
    }
    pub(crate) fn acabar(&self, seg: Option<Segmento>) {
        let mut e = self.lock();
        if let Some(s) = seg {
            e.segmentos.push(s);
        }
        e.ocupado = false;
        self.hecho.notify_all();
    }
    pub(crate) fn anticipar(&self, inicio: usize, hasta: usize, umbral: f32) {
        self.lock().anticipando = Some((inicio, hasta, umbral));
    }
    pub(crate) fn anticipado(&self, a: Option<Anticipado>) {
        let mut e = self.lock();
        if a.is_some() {
            e.anticipado = a;
        }
        e.anticipando = None;
        self.hecho.notify_all();
    }
    /// Los segmentos confirmados; si hay una confirmación a medias, la espera (como mucho `plazo`): ese
    /// trabajo hace falta igual. Si no llega a tiempo, `None`: se transcribe todo como siempre.
    pub fn al_soltar(&self, plazo: Duration) -> Option<Vec<Segmento>> {
        self.al_soltar_con(plazo, &[]).map(|(s, _)| s)
    }

    /// Como `al_soltar`, y además el texto anticipado de la cola si vale para `audio` (todo el dictado a
    /// 16 kHz): empieza donde acaba el último segmento y después no se habló. Si se está anticipando y no
    /// se habló después, se espera (es el texto final); si se habló, no.
    pub fn al_soltar_con(&self, plazo: Duration, audio: &[f32]) -> Option<(Vec<Segmento>, Option<String>)> {
        let fin = Instant::now() + plazo;
        let mut e = self.lock();
        loop {
            let fin_segs = e.segmentos.last().map_or(0, |s| s.fin);
            let sirve = |inicio: usize, hasta: usize, umbral: f32| inicio == fin_segs && hasta <= audio.len() && !hay_voz(&audio[hasta..], umbral);
            let esperar_anticipo = e.anticipando.is_some_and(|(i, h, u)| sirve(i, h, u));
            if !e.ocupado && !esperar_anticipo {
                let cola = e.anticipado.as_ref().filter(|a| sirve(a.inicio, a.hasta, a.umbral)).map(|a| a.crudo.clone());
                return Some((e.segmentos.clone(), cola));
            }
            let falta = fin.saturating_duration_since(Instant::now());
            if falta.is_zero() {
                return if e.ocupado { None } else { Some((e.segmentos.clone(), None)) };
            }
            e = self.hecho.wait_timeout(e, falta).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

/// Transcribe un segmento (audio a 16 kHz) y devuelve lo oído, o `None` si falla o el motor está
/// ocupado (se reintenta en el siguiente tick).
pub type Confirmar = Box<dyn FnMut(&[f32]) -> Option<String> + Send>;

/// ¿Cubren los segmentos el audio desde 0, seguidos, sin huecos ni solapes, y dentro de `total`?
pub fn segmentos_validos(segs: &[Segmento], total: usize) -> bool {
    let mut fin = 0;
    for s in segs {
        if s.inicio != fin || s.fin <= s.inicio {
            return false;
        }
        fin = s.fin;
    }
    fin <= total
}

/// La lógica de cada tick, sin hilos ni micrófono.
struct Vivo<D> {
    fijador: Fijador,
    /// Texto de cada tramo fijado, en orden.
    piezas: Vec<String>,
    /// Fin (muestras desde lo fijado) del último corte que salió sin texto con voz: no se vuelve a
    /// transcribir hasta que el corte avance (`MARGEN_CORTE`).
    vacio: Option<usize>,
    vista: Vista,
    decodificar: D,
    /// Dónde está cada tramo fijado (las pruebas miran que cubran el audio sin huecos).
    #[cfg(test)]
    tramos: Vec<(usize, usize)>,
}

impl<D: FnMut(&[f32]) -> Option<Result<Timed, String>>> Vivo<D> {
    fn new(decodificar: D) -> Self {
        Self {
            fijador: Fijador::default(),
            piezas: Vec::new(),
            vacio: None,
            vista: Vista::default(),
            decodificar,
            #[cfg(test)]
            tramos: Vec::new(),
        }
    }

    /// `None` = no hay texto: el motor está ocupado, falló o ya se soltó el atajo (`parar`, que se mira
    /// justo antes de llamar al motor: tras soltar no empieza ninguna decodificación).
    fn transcribir(&mut self, samples: &[f32], parar: &AtomicBool) -> Option<Timed> {
        if callado(samples) {
            return Some(Timed::default());
        }
        if parar.load(Ordering::SeqCst) {
            return None;
        }
        match (self.decodificar)(samples) {
            Some(Ok(t)) => Some(t),
            Some(Err(e)) => {
                eprintln!("[vivo] tick sin texto: {e}");
                None
            }
            None => None,
        }
    }

    /// Un tick: `cola` es el audio (16 kHz) desde lo fijado hasta ahora. Devuelve si cambió la vista.
    fn tick(&mut self, cola: &[f32], parar: &AtomicBool) -> bool {
        if cola.len() < FRAME {
            return false;
        }
        let antes = self.vista.clone();
        match self.fijador.corte(cola) {
            Some(Corte { n: corte, pausa }) => {
                let tramo = &cola[..corte];
                // Un corte en una pausa siempre sigue a voz (`VOZ_MINIMA`). Uno forzado puede ser solo
                // ruido de fondo: se fija sin transcribirlo, que Whisper ahí inventa «Gracias.».
                let con_voz = pausa || self.fijador.voz_hasta(tramo).is_some();
                let tope = corte >= TRAMO_MAXIMO;
                // Un tramo con voz que salió sin texto no se fija (se perdería de la vista): va con lo que
                // sigue. Tampoco se vuelve a transcribir hasta que el corte avance; pasado el tope, sí se fija.
                if con_voz && !tope && self.vacio.is_some_and(|v| corte < v + MARGEN_CORTE) {
                    self.provisional(cola, parar);
                    return self.vista != antes;
                }
                let texto = if con_voz {
                    // Si el motor está ocupado no se fija nada: se reintenta en el siguiente tick.
                    let Some(timed) = self.transcribir(tramo, parar) else { return false };
                    timed.text
                } else {
                    String::new()
                };
                if con_voz && !tope && texto.trim().is_empty() {
                    self.vacio = Some(corte);
                    self.provisional(cola, parar);
                    return self.vista != antes;
                }
                self.vacio = None;
                #[cfg(test)]
                self.tramos.push((self.fijador.fijado(), self.fijador.fijado() + corte));
                self.piezas.push(texto);
                self.fijador.fijar(corte);
                let fixed = self.piezas.iter().fold(String::new(), |a, t| unir(&a, t));
                self.vista = Vista { fixed, partial: String::new() };
                let resto = &cola[corte..];
                if resto.len() >= PROVISIONAL_MINIMO {
                    self.provisional(resto, parar);
                }
            }
            None => self.provisional(cola, parar),
        }
        self.vista != antes
    }

    /// Lo provisional: solo la voz, sin el silencio de detrás, y nada si aún no hay voz. Whisper
    /// inventa frases en el silencio («Gracias.», «¿Qué?»: prueba de Txonso del 19-nov): con la cola
    /// recién empezada o tras una pausa, la barra enseñaba palabras que nadie había dicho.
    fn provisional(&mut self, cola: &[f32], parar: &AtomicBool) {
        match self.fijador.voz_hasta(cola) {
            None => self.vista.partial.clear(),
            Some(fin) => {
                if let Some(t) = self.transcribir(&cola[..(fin + COLA_VISTA).min(cola.len())], parar) {
                    self.vista.partial = t.text;
                }
            }
        }
    }

    #[cfg(test)]
    fn fijado(self) -> Fijado {
        let piezas = self.tramos.iter().zip(self.piezas).map(|(&(inicio, fin), texto)| Pieza { inicio, fin, texto }).collect();
        Fijado { fin: self.fijador.fijado(), piezas }
    }
}

/// Un tramo fijado (pruebas): su texto y dónde está (muestras a 16 kHz desde el principio).
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct Pieza {
    pub inicio: usize,
    pub fin: usize,
    pub texto: String,
}

/// Lo fijado al acabar (pruebas): las piezas, seguidas y sin huecos desde 0 hasta `fin`.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct Fijado {
    pub piezas: Vec<Pieza>,
    pub fin: usize,
}

/// Muestra nativa del micrófono que corresponde a la muestra `i` a 16 kHz.
fn nativa(i: usize, rate: u32) -> usize {
    (i as u64 * rate as u64 / RATE as u64) as usize
}

/// El hilo del texto en vivo de un dictado. Al soltarla (`drop`) deja de transcribir: no espera al tick
/// que estuviera a medias (su texto ya no se enseña) ni empieza otra decodificación.
pub struct Sesion {
    parar: Arc<AtomicBool>,
    confirmados: Arc<Confirmados>,
}

/// Qué hace el hilo además de la vista previa.
pub struct Opciones {
    /// Enseñar la vista previa (texto en vivo). Sin ella, el hilo solo confirma segmentos.
    pub vista: bool,
    /// Confirmar segmentos largos (`None` = no; así era antes de BELTR-PRT-16).
    pub confirmar: Option<Confirmar>,
    /// Longitudes del segmento (`SEG_MINIMO`/`SEG_TOPE`, o las de Whisper).
    pub minimo: usize,
    pub tope: usize,
    /// Anticipar el texto de la cola al dejar de hablar (`Anticipado`; `None` = no). Corre sin el turno
    /// del motor (como un tick con Parakeet): no hace esperar a nadie.
    pub anticipar: Option<Confirmar>,
}

impl Default for Opciones {
    fn default() -> Self {
        Self { vista: true, confirmar: None, minimo: SEG_MINIMO, tope: SEG_TOPE, anticipar: None }
    }
}

impl Sesion {
    /// Lo confirmado por este dictado (lo lee el hilo que transcribe al soltar).
    pub fn confirmados(&self) -> Arc<Confirmados> {
        self.confirmados.clone()
    }

    pub fn empezar(tap: Tap, preparar: Box<dyn FnOnce() + Send>, decodificar: Decodificar, al_cambiar: Box<dyn Fn(&Vista) + Send>) -> Option<Self> {
        Self::empezar_con(tap, preparar, decodificar, al_cambiar, Opciones::default())
    }

    /// Arranca el hilo: cada `TICK` lee lo grabado desde lo fijado (`tap`), lo transcribe con
    /// `decodificar` y llama a `al_cambiar` si cambió lo que se ve. `preparar` corre antes, en el hilo.
    pub fn empezar_con(tap: Tap, preparar: Box<dyn FnOnce() + Send>, decodificar: Decodificar, al_cambiar: Box<dyn Fn(&Vista) + Send>, opciones: Opciones) -> Option<Self> {
        let parar = Arc::new(AtomicBool::new(false));
        let flag = parar.clone();
        let confirmados = Arc::new(Confirmados::default());
        let conf = confirmados.clone();
        let Opciones { vista: con_vista, mut confirmar, minimo, tope, mut anticipar } = opciones;
        std::thread::Builder::new()
            .name("pronto-vivo".into())
            .spawn(move || {
                preparar();
                let mut vivo = Vivo::new(decodificar);
                let mut cortador = Fijador::default();
                // Hasta dónde llegaba la voz la última vez que se anticipó (no se repite sin voz nueva).
                let mut voz_anticipada = 0usize;
                let t0 = Instant::now();
                let mut siguiente = t0 + TICK;
                loop {
                    while !flag.load(Ordering::SeqCst) && Instant::now() < siguiente {
                        std::thread::sleep(siguiente.saturating_duration_since(Instant::now()).min(Duration::from_millis(10)));
                    }
                    if flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let rate = tap.rate();
                    // Primero, confirmar un segmento si ya toca (en este hilo: no pelea con los ticks por el motor).
                    if let Some(confirmar) = confirmar.as_mut() {
                        let pendiente = audio::resample_to_16k(&tap.since(nativa(cortador.fijado(), rate)), rate);
                        if let Some(n) = cortador.corte_segmento(&pendiente, minimo, tope) {
                            conf.empezar();
                            let inicio = cortador.fijado();
                            let seg = confirmar(&pendiente[..n]).map(|crudo| Segmento { inicio, fin: inicio + n, crudo });
                            if seg.is_some() {
                                cortador.fijar(n);
                            }
                            conf.acabar(seg);
                        } else if let Some(anticipar) = anticipar.as_mut().filter(|_| !flag.load(Ordering::SeqCst) && pendiente.len() < tope) {
                            // Dejó de hablar: se anticipa el texto de lo pendiente, una vez por cada voz nueva.
                            let mut f = cortador.clone();
                            if let Some((voz_fin, umbral)) = f.fin_de_voz(&pendiente, FIN_PAUSA) {
                                let inicio = cortador.fijado();
                                if inicio + voz_fin > voz_anticipada {
                                    voz_anticipada = inicio + voz_fin;
                                    let hasta = inicio + pendiente.len();
                                    conf.anticipar(inicio, hasta, umbral);
                                    let a = anticipar(&pendiente).map(|crudo| Anticipado { inicio, hasta, crudo, umbral });
                                    conf.anticipado(a);
                                }
                            }
                        }
                    }
                    if con_vista && !flag.load(Ordering::SeqCst) {
                        let cola = audio::resample_to_16k(&tap.since(nativa(vivo.fijador.fijado(), rate)), rate);
                        if vivo.tick(&cola, &flag) && !flag.load(Ordering::SeqCst) {
                            al_cambiar(&vivo.vista);
                        }
                    }
                    siguiente = proximo_tick(t0, Instant::now(), TICK);
                }
            })
            .map_err(|e| eprintln!("[vivo] no se pudo lanzar el hilo: {e}"))
            .ok()?;
        Some(Self { parar, confirmados })
    }
}

impl Drop for Sesion {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Voz sintética: un tono de `ms` milisegundos (amplitud de voz normal).
    pub fn voz(ms: usize) -> Vec<f32> {
        (0..ms * RATE / 1000).map(|i| (i as f32 * 0.13).sin() * 0.1).collect()
    }

    /// Silencio con un ruido de fondo muy bajo.
    pub fn silencio(ms: usize) -> Vec<f32> {
        (0..ms * RATE / 1000).map(|i| (i as f32 * 1.7).sin() * 0.0008).collect()
    }

    fn junta(trozos: &[Vec<f32>]) -> Vec<f32> {
        trozos.concat()
    }

    fn ms(n: usize) -> usize {
        n * RATE / 1000
    }

    /// Muestras que fija `corte` (solo cortes en una pausa).
    fn en_pausa(f: &mut Fijador, cola: &[f32]) -> Option<usize> {
        f.corte(cola).map(|c| {
            assert!(c.pausa, "se esperaba un corte en una pausa: {c:?}");
            c.n
        })
    }

    #[test]
    fn sin_pausa_no_se_fija_nada() {
        let mut f = Fijador::default();
        assert_eq!(f.corte(&voz(3000)), None);
        assert_eq!(f.corte(&junta(&[voz(1700), silencio(450), voz(1700)])), None, "450 ms (una coma) no fijan");
    }

    #[test]
    fn una_pausa_tras_voz_fija_hasta_su_mitad() {
        let mut f = Fijador::default();
        let audio = junta(&[voz(2000), silencio(800), voz(500)]);
        let corte = en_pausa(&mut f, &audio).expect("tenía que fijar en la pausa");
        assert!(corte >= ms(2300) && corte <= ms(2500), "el corte va hacia la mitad de la pausa, no en un borde: {corte}");
    }

    #[test]
    fn la_pausa_que_aun_dura_tambien_fija() {
        // El usuario se ha callado ahora mismo: ya lleva 620 ms en silencio.
        let mut f = Fijador::default();
        let corte = en_pausa(&mut f, &junta(&[voz(2000), silencio(620)])).expect("pausa abierta");
        assert!(corte > ms(2000) && corte < ms(2620));
    }

    #[test]
    fn silencio_inicial_sin_voz_no_se_fija_solo() {
        let mut f = Fijador::default();
        assert_eq!(f.corte(&silencio(3000)), None, "sin voz no hay nada que fijar todavía");
        assert_eq!(f.corte(&junta(&[silencio(1000), voz(100), silencio(800)])), None, "un chasquido no es voz");
    }

    #[test]
    fn un_tramo_demasiado_corto_espera_a_juntarse_con_el_siguiente() {
        let mut f = Fijador::default();
        assert_eq!(f.corte(&junta(&[voz(500), silencio(700)])), None, "media palabra no se fija sola");
        let corte = en_pausa(&mut f, &junta(&[voz(500), silencio(700), voz(1200), silencio(700)])).expect("ya hay tramo");
        assert!(corte > ms(2400), "se fija hasta la última pausa: {corte}");
    }

    #[test]
    fn se_fija_hasta_la_ultima_pausa() {
        let mut f = Fijador::default();
        let audio = junta(&[voz(1600), silencio(700), voz(1600), silencio(700), voz(300)]);
        let corte = en_pausa(&mut f, &audio).expect("dos pausas");
        assert!(corte > ms(3900) && corte < ms(4600), "la segunda pausa: {corte}");
    }

    #[test]
    fn sin_pausas_se_corta_por_lo_mas_silencioso_antes_del_tope() {
        let mut f = Fijador::default();
        // 9 s hablando sin pausas largas, con un bajón de 100 ms a los 6 s.
        let mut audio = junta(&[voz(6000), silencio(100), voz(2900)]);
        assert!(audio.len() > TRAMO_MAXIMO);
        let c = f.corte(&audio).expect("pasado el tope se corta");
        assert!(!c.pausa, "es un corte forzado");
        assert!(c.n > ms(6000) && c.n <= ms(6100), "corta en el bajón, no a mitad de voz: {}", c.n);
        // Sin bajón, corta igualmente antes del último medio segundo.
        audio = voz(9000);
        let c = Fijador::default().corte(&audio).expect("tope");
        assert!(c.n <= audio.len() - MARGEN_CORTE && c.n >= audio.len() / 2);
    }

    #[test]
    fn con_ruido_de_fondo_la_pausa_se_sigue_viendo() {
        // Ventilador: ruido 10 veces más alto que el silencio limpio; la voz sigue muy por encima.
        let ruido = |n: usize| -> Vec<f32> { (0..ms(n)).map(|i| (i as f32 * 2.3).sin() * 0.008).collect() };
        let mut f = Fijador::default();
        let audio = junta(&[ruido(500), voz(2000), ruido(700), voz(500)]);
        let corte = en_pausa(&mut f, &audio).expect("la pausa con ruido es pausa");
        assert!(corte > ms(2500) && corte < ms(3200), "{corte}");
    }

    #[test]
    fn el_tick_que_llega_tarde_se_salta() {
        let t0 = Instant::now();
        assert_eq!(proximo_tick(t0, t0, TICK), t0 + TICK);
        assert_eq!(proximo_tick(t0, t0 + Duration::from_millis(100), TICK), t0 + TICK);
        // Un tick que acaba a los 1,5 s (tardó más de 0,7 s): el de 1,4 s se salta y toca el de 2,1 s.
        assert_eq!(proximo_tick(t0, t0 + Duration::from_millis(1500), TICK), t0 + TICK * 3);
    }

    /// Decodificador falso: cuenta las llamadas y devuelve «pN» con la duración del trozo en ms.
    fn falso(llamadas: Arc<std::sync::Mutex<Vec<usize>>>) -> impl FnMut(&[f32]) -> Option<Result<Timed, String>> {
        move |s: &[f32]| {
            llamadas.lock().unwrap().push(s.len());
            Some(Ok(Timed { text: format!("p{}", s.len() * 1000 / RATE), tokens: vec![] }))
        }
    }

    /// Simula un dictado: un tick cada `TICK` de audio, con `cola` = lo grabado desde lo fijado.
    fn simular<D: FnMut(&[f32]) -> Option<Result<Timed, String>>>(audio: &[f32], d: D) -> (Fijado, Vec<Vista>) {
        super::tests_simular(audio, d)
    }

    #[test]
    fn lo_fijado_cubre_el_audio_sin_huecos_y_no_se_vuelve_a_transcribir() {
        let audio = junta(&[voz(1800), silencio(800), voz(1800), silencio(900), voz(1000), silencio(300), voz(800)]);
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (fijado, vistas) = simular(&audio, falso(llamadas.clone()));
        assert!(fijado.piezas.len() >= 2, "dos pausas, dos tramos: {:?}", fijado.piezas.iter().map(|p| (p.inicio, p.fin)).collect::<Vec<_>>());
        let mut fin = 0;
        for p in &fijado.piezas {
            assert_eq!(p.inicio, fin, "hueco o solape entre tramos");
            assert!(p.fin > p.inicio);
            fin = p.fin;
        }
        assert_eq!(fin, fijado.fin);
        assert!(fijado.fin < audio.len(), "lo último (sin pausa detrás) queda para el final");
        // Lo provisional se enseña mientras se habla y lo fijado solo crece.
        assert!(vistas.iter().any(|v| !v.partial.is_empty()));
        let mut largo = 0;
        for v in &vistas {
            assert!(v.fixed.len() >= largo, "lo fijado no puede encoger: {vistas:?}");
            largo = v.fixed.len();
        }
        // Ninguna llamada transcribe más que el tramo máximo más un tick.
        assert!(llamadas.lock().unwrap().iter().all(|&n| n <= TRAMO_MAXIMO + RATE));
    }

    #[test]
    fn con_el_motor_ocupado_no_se_fija_ni_se_pierde_nada() {
        let audio = junta(&[voz(1800), silencio(800), voz(1800), silencio(900)]);
        let (fijado, vistas) = simular(&audio, |_: &[f32]| None);
        assert_eq!(fijado.fin, 0, "sin motor no se fija: todo queda para el final");
        assert!(fijado.piezas.is_empty() && vistas.is_empty());
        // Ocupado al principio y libre después: lo fijado sigue empezando en 0.
        let mut n = 0;
        let (fijado, _) = simular(&audio, move |s: &[f32]| {
            n += 1;
            (n > 3).then(|| Ok(Timed { text: format!("t{}", s.len()), tokens: vec![] }))
        });
        assert_eq!(fijado.piezas.first().map(|p| p.inicio), Some(0));
    }

    #[test]
    fn un_error_del_motor_no_fija_el_tramo() {
        let audio = junta(&[voz(1800), silencio(800), voz(600)]);
        let (fijado, _) = simular(&audio, |_: &[f32]| Some(Err("falló".to_string())));
        assert_eq!(fijado.fin, 0);
    }

    #[test]
    fn el_silencio_mudo_no_llama_al_motor() {
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (_, vistas) = simular(&vec![0.0; ms(3000)], falso(llamadas.clone()));
        assert!(llamadas.lock().unwrap().is_empty(), "transcribió silencio digital");
        assert!(vistas.is_empty());
    }

    /// Prueba de Txonso del 19-nov con Whisper: al empezar y tras cada pausa la barra enseñaba
    /// «Gracias.», que nadie había dicho. Lo provisional solo transcribe voz, sin el silencio de detrás.
    #[test]
    fn la_vista_previa_no_transcribe_silencio_ni_lo_que_va_detras_de_la_voz() {
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        // Primero 1,4 s callado (con ruido de fondo, no silencio digital): dos ticks sin nada que enseñar.
        let audio = junta(&[silencio(1400), voz(1000), silencio(1700)]);
        let (fijado, vistas) = simular(&audio, falso(llamadas.clone()));
        let l = llamadas.lock().unwrap().clone();
        assert!(!l.is_empty(), "con voz sí se transcribe");
        // Lo provisional nunca llega más de 200 ms (y un tramo) más allá del final de la voz (2,4 s); el
        // tramo que se fija sí lleva media pausa, como siempre.
        let fijados: Vec<usize> = fijado.piezas.iter().map(|p| p.fin - p.inicio).collect();
        assert!(l.iter().all(|&n| n <= ms(2400) + COLA_VISTA + FRAME || fijados.contains(&n)), "transcribió el silencio de detrás: {l:?}");
        assert!(vistas.first().is_some_and(|v| !v.partial.is_empty()), "la primera vista ya trae voz: {vistas:?}");
        // Tras fijar en la pausa, la cola en silencio no se transcribe y lo provisional se vacía.
        let audio = junta(&[voz(1800), silencio(1000), silencio(1400)]);
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (fijado, vistas) = simular(&audio, falso(llamadas.clone()));
        assert_eq!(fijado.piezas.len(), 1);
        assert_eq!(vistas.last().map(|v| v.partial.as_str()), Some(""), "{vistas:?}");
        // Las únicas transcripciones cortas son las de mientras se hablaba (0,7 y 1,4 s): ninguna de la cola callada.
        let cortas: Vec<usize> = llamadas.lock().unwrap().iter().copied().filter(|&n| n < ms(1800)).collect();
        assert_eq!(cortas, vec![ms(700), ms(1400)], "transcribió una cola sin voz");
    }

    /// Hallazgo 1 (revisión de ea34a16): un tramo con voz que el motor devolvía sin texto se fijaba vacío
    /// y esas palabras ya no salían en la vista. No se fija: va con lo que sigue en el siguiente tick.
    #[test]
    fn un_tramo_con_voz_que_sale_sin_texto_no_se_fija() {
        let audio = junta(&[voz(1800), silencio(800), voz(1800), silencio(2000)]);
        // El motor no saca nada de los trozos cortos (hasta 2,5 s: el primer tramo con su media pausa).
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let l2 = llamadas.clone();
        let (fijado, vistas) = simular(&audio, move |s: &[f32]| {
            l2.lock().unwrap().push(s.len());
            Some(Ok(Timed { text: if s.len() <= ms(2500) { String::new() } else { format!("p{}", s.len() * 1000 / RATE) }, tokens: vec![] }))
        });
        assert!(!fijado.piezas.is_empty(), "lo que sigue sí se fija");
        assert!(fijado.piezas.iter().all(|p| !p.texto.is_empty()), "se fijó un tramo con voz sin texto: {:?}", fijado.piezas);
        assert_eq!(fijado.piezas[0].inicio, 0, "el primer tramo va con el segundo: nada queda fuera");
        assert!(vistas.last().is_some_and(|v| !v.fixed.is_empty()), "{vistas:?}");
        // El corte que salió vacío no se transcribe en cada tick: solo cuando avanza (una pausa nueva).
        let fijo = fijado.piezas[0].fin;
        let cortes_vacios = llamadas.lock().unwrap().iter().filter(|&&n| n > ms(2000) && n <= ms(2500)).count();
        assert!(cortes_vacios <= 2, "el mismo corte vacío se transcribió {cortes_vacios} veces (fijo hasta {fijo})");
    }

    /// Hallazgo 5: pasado el tope sin pausas, un corte forzado de solo ruido de fondo se transcribía, y
    /// Whisper ahí inventa «Gracias.». Se fija sin llamar al motor.
    #[test]
    fn un_corte_forzado_sin_voz_no_llama_al_motor() {
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (fijado, vistas) = simular(&silencio(10_000), falso(llamadas.clone()));
        assert!(fijado.fin > 0, "pasado el tope se fija igual");
        assert!(llamadas.lock().unwrap().is_empty(), "transcribió ruido de fondo: {:?}", llamadas.lock().unwrap());
        assert!(vistas.iter().all(|v| v.fixed.is_empty() && v.partial.is_empty()), "{vistas:?}");
        // Con voz, el corte forzado sí se transcribe.
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (fijado, _) = simular(&voz(10_000), falso(llamadas.clone()));
        assert!(fijado.piezas.first().is_some_and(|p| !p.texto.is_empty()));
    }

    /// Regla 3: tras soltar (`parar`), ningún tick llama al motor, ni para fijar ni para lo provisional.
    #[test]
    fn tras_soltar_no_se_llama_al_motor() {
        let audio = junta(&[voz(1800), silencio(800), voz(1800), silencio(900), voz(800)]);
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut vivo = Vivo::new(falso(llamadas.clone()));
        let parar = AtomicBool::new(true);
        assert!(!vivo.tick(&audio, &parar));
        assert!(!vivo.tick(&audio[..ms(1500)], &parar));
        assert!(llamadas.lock().unwrap().is_empty(), "llamó al motor tras soltar: {:?}", llamadas.lock().unwrap());
        assert_eq!(vivo.fijador.fijado(), 0, "sin transcribir no se fija nada");
    }

    /// Hallazgo 3 y regla 3: al soltar el atajo la sesión para sin esperar al tick a medias (el dictado
    /// final no lo necesita) y ese tick no empieza otra decodificación (la de lo provisional tras fijar),
    /// ni avisa a la barra.
    #[test]
    fn al_soltar_no_se_espera_al_tick_a_medias_ni_se_decodifica_mas() {
        // 6 s ya grabados a 48 kHz: el primer tick fija en una pausa y, detrás, querría lo provisional.
        let a16 = junta(&[voz(1800), silencio(800), voz(1800), silencio(900), voz(800)]);
        let a48: Vec<f32> = a16.iter().flat_map(|&s| [s, s, s]).collect();
        let tap = Tap::from_samples(a48, 48_000);
        let (empieza_tx, empieza) = std::sync::mpsc::channel::<()>();
        let llamadas = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let l2 = llamadas.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Vista>();
        let preparado = Arc::new(AtomicBool::new(false));
        let p2 = preparado.clone();
        let s = Sesion::empezar(
            tap,
            Box::new(move || p2.store(true, Ordering::SeqCst)),
            Box::new(move |s: &[f32]| {
                l2.fetch_add(1, Ordering::SeqCst);
                let _ = empieza_tx.send(());
                std::thread::sleep(Duration::from_millis(400));
                Some(Ok(Timed { text: format!("t{}", s.len() / 1600), tokens: vec![] }))
            }),
            Box::new(move |v: &Vista| {
                let _ = tx.send(v.clone());
            }),
        )
        .expect("hilo");
        empieza.recv_timeout(Duration::from_secs(5)).expect("el primer tick transcribe");
        drop(s);
        assert!(preparado.load(Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(400) + TICK + Duration::from_millis(200));
        assert_eq!(llamadas.load(Ordering::SeqCst), 1, "tras soltar se decodificó otra vez");
        assert_eq!(rx.try_iter().count(), 0, "siguió avisando a la barra tras soltar");
    }


    // ------------------------------------------------ segmentos confirmados (BELTR-PRT-16)

    /// Regla 3: un dictado más corto que el mínimo no se confirma nunca (sale exactamente como antes).
    #[test]
    fn un_dictado_de_15_s_no_confirma_segmentos() {
        let a = junta(&[voz(4000), silencio(900), voz(5000), silencio(900), voz(4000)]);
        let mut f = Fijador::default();
        assert_eq!(f.corte_segmento(&a, SEG_MINIMO, SEG_TOPE), None);
    }

    /// El corte cae en la primera pausa de verdad a partir de los 20 s, en su mitad.
    #[test]
    fn el_segmento_se_corta_en_la_primera_pausa_tras_el_minimo() {
        let a = junta(&[voz(9000), silencio(800), voz(11_500), silencio(800), voz(6000), silencio(900), voz(3000)]);
        let mut f = Fijador::default();
        let n = f.corte_segmento(&a, SEG_MINIMO, SEG_TOPE).expect("corte");
        // La segunda pausa va de 21,3 s a 22,1 s: el corte, por su mitad (con margen de un tramo).
        assert!(n > ms(21_500) && n < ms(21_900), "corte en {} ms", n * 1000 / RATE);
        assert!(a[n - ms(200)..n + ms(200)].iter().all(|x| x.abs() < 0.01), "el corte no cae en silencio");
    }

    /// Una pausa corta (una respiración) no corta: haría falta `PAUSA` (600 ms).
    #[test]
    fn una_respiracion_no_corta_el_segmento() {
        let a = junta(&[voz(21_000), silencio(300), voz(10_000)]);
        let mut f = Fijador::default();
        assert_eq!(f.corte_segmento(&a, SEG_MINIMO, SEG_TOPE), None);
    }

    /// Sin pausas, al tope (40 s) se corta en lo más silencioso de los últimos 6 s.
    #[test]
    fn sin_pausas_el_segmento_se_corta_al_tope() {
        let a = junta(&[voz(36_000), silencio(150), voz(8000)]);
        let mut f = Fijador::default();
        let n = f.corte_segmento(&a, SEG_MINIMO, SEG_TOPE).expect("corte forzado");
        assert!(n <= SEG_TOPE && n >= ms(34_000), "corte en {} ms", n * 1000 / RATE);
        assert!(n >= ms(36_000) && n <= ms(36_160), "no cortó en lo más silencioso: {} ms", n * 1000 / RATE);
        assert_eq!(Fijador::default().corte_segmento(&a[..ms(39_000)], SEG_MINIMO, SEG_TOPE), None, "antes del tope, sin pausa, se espera");
    }

    #[test]
    fn los_segmentos_tienen_que_cubrir_el_audio_seguidos() {
        let s = |i, f| Segmento { inicio: i, fin: f, crudo: "x".into() };
        assert!(segmentos_validos(&[s(0, 10), s(10, 30)], 40));
        assert!(segmentos_validos(&[], 40));
        assert!(!segmentos_validos(&[s(0, 10), s(12, 30)], 40), "hueco");
        assert!(!segmentos_validos(&[s(0, 10), s(8, 30)], 40), "solape");
        assert!(!segmentos_validos(&[s(5, 10)], 40), "no empieza en 0");
        assert!(!segmentos_validos(&[s(0, 50)], 40), "más allá del audio");
    }

    /// La sesión confirma segmentos seguidos mientras se habla, también sin vista previa, y al soltar
    /// se leen todos; la cola queda sin confirmar.
    #[test]
    fn la_sesion_confirma_segmentos_seguidos_y_deja_la_cola() {
        let a16 = junta(&[voz(12_000), silencio(800), voz(9000), silencio(800), voz(12_000), silencio(800), voz(9000), silencio(800), voz(5000)]);
        let total = a16.len();
        let a48: Vec<f32> = a16.iter().flat_map(|&s| [s, s, s]).collect();
        let tap = Tap::from_samples(a48, 48_000);
        let llamadas = Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
        let l2 = llamadas.clone();
        let confirmar: Confirmar = Box::new(move |s: &[f32]| {
            l2.lock().unwrap().push(s.len());
            Some(format!("seg{}", l2.lock().unwrap().len()))
        });
        let vistas = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let v2 = vistas.clone();
        let sesion = Sesion::empezar_con(
            tap,
            Box::new(|| {}),
            Box::new(move |_s: &[f32]| {
                v2.fetch_add(1, Ordering::SeqCst);
                Some(Ok(Timed { text: "v".into(), tokens: vec![] }))
            }),
            Box::new(|_| {}),
            Opciones { vista: false, confirmar: Some(confirmar), minimo: SEG_MINIMO, tope: SEG_TOPE, anticipar: None },
        )
        .expect("hilo");
        let conf = sesion.confirmados();
        std::thread::sleep(TICK * 4 + Duration::from_millis(300));
        drop(sesion);
        let segs = conf.al_soltar(Duration::from_secs(2)).expect("sin confirmación a medias");
        assert_eq!(segs.len(), 2, "{segs:?}");
        assert!(segmentos_validos(&segs, total));
        assert_eq!(segs[0].crudo, "seg1");
        // Primer corte en la pausa de 21,8-22,6 s; el segundo, 20 s después como mínimo.
        assert!(segs[0].fin > ms(22_000) && segs[0].fin < ms(22_400), "corte 1 en {} ms", segs[0].fin * 1000 / RATE);
        assert!(segs[1].fin - segs[1].inicio >= SEG_MINIMO);
        assert!(total - segs[1].fin > ms(4000), "la cola queda para soltar");
        assert_eq!(vistas.load(Ordering::SeqCst), 0, "sin vista previa no se llama al motor para la vista");
    }

    /// Al dejar de hablar se anticipa el texto de lo pendiente; al soltar vale si no se habló después.
    #[test]
    fn al_dejar_de_hablar_se_anticipa_el_texto_y_vale_si_no_se_hablo_despues() {
        let a16 = junta(&[silencio(400), voz(3000), silencio(1200)]);
        let tap = Tap::from_samples(a16.clone(), 16_000);
        let llamadas = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let l2 = llamadas.clone();
        let anticipar: Confirmar = Box::new(move |s: &[f32]| {
            l2.fetch_add(1, Ordering::SeqCst);
            Some(format!("todo {}", s.len()))
        });
        let sesion = Sesion::empezar_con(
            tap,
            Box::new(|| {}),
            Box::new(|_s: &[f32]| None),
            Box::new(|_| {}),
            Opciones { vista: false, confirmar: Some(Box::new(|_s: &[f32]| None)), minimo: SEG_MINIMO, tope: SEG_TOPE, anticipar: Some(anticipar) },
        )
        .expect("hilo");
        let conf = sesion.confirmados();
        std::thread::sleep(TICK * 3 + Duration::from_millis(200));
        drop(sesion);
        assert_eq!(llamadas.load(Ordering::SeqCst), 1, "una sola vez por cada voz nueva");
        // Sin voz después: el texto anticipado es la cola.
        let (segs, cola) = conf.al_soltar_con(Duration::from_secs(1), &a16).expect("algo");
        assert!(segs.is_empty());
        assert_eq!(cola.as_deref(), Some(format!("todo {}", a16.len()).as_str()));
        // Con un poco más de silencio, también.
        let mas = junta(&[a16.clone(), silencio(500)]);
        assert!(conf.al_soltar_con(Duration::from_secs(1), &mas).unwrap().1.is_some());
        // Si se habló después, no vale: se transcribe como siempre. También con una palabra corta («sí»).
        let otra = junta(&[a16.clone(), voz(800)]);
        assert_eq!(conf.al_soltar_con(Duration::from_secs(1), &otra).unwrap().1, None);
        let corta = junta(&[a16.clone(), silencio(200), voz(120), silencio(300)]);
        assert_eq!(conf.al_soltar_con(Duration::from_secs(1), &corta).unwrap().1, None, "un «sí» de 120 ms tras la pausa no se pierde");
    }

    /// Al soltar se espera la confirmación a medias (hace falta igual), pero no más del plazo.
    #[test]
    fn al_soltar_se_espera_la_confirmacion_a_medias_con_plazo() {
        let c = Arc::new(Confirmados::default());
        c.empezar();
        let c2 = c.clone();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            c2.acabar(Some(Segmento { inicio: 0, fin: 10, crudo: "hola".into() }));
        });
        let t = Instant::now();
        let segs = c.al_soltar(Duration::from_secs(2)).expect("llega");
        assert_eq!(segs.len(), 1);
        assert!(t.elapsed() >= Duration::from_millis(120));
        h.join().unwrap();
        c.empezar();
        assert_eq!(c.al_soltar(Duration::from_millis(100)), None, "pasado el plazo, camino de siempre");
    }
}

/// Regla 3 (velocidad) con texto en vivo. La lanza `tests/no-se-negocia/03-velocidad.test.mjs` con
/// `PRONTO_NSN_WAV` y `PRONTO_NSN_MODELO`, como `nsn_velocidad_parakeet_dictado_15s` (su nombre no
/// contiene «nsn_velocidad» para que no corran a la vez y se roben la CPU). Simula el dictado con el
/// modelo real (un tick cada 0,7 s de audio mientras «se habla») y al «soltar» mide hasta el texto final
/// (`state::transcribe_heard` + tubería, como el dictado) en el peor caso: el tick más largo acaba de
/// empezar. Lo compara con el mismo camino sin texto en vivo, y el texto tiene que ser el mismo.
#[cfg(test)]
mod nsn_vivo {
    use super::*;
    use crate::asr::Engine;

    fn mediana(v: &[f64]) -> f64 {
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        if s.len() % 2 == 1 { s[s.len() / 2] } else { (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0 }
    }

    #[test]
    fn nsn_vivo_soltar_hasta_texto() {
        let (Ok(wav), Ok(dir)) = (std::env::var("PRONTO_NSN_WAV"), std::env::var("PRONTO_NSN_MODELO")) else {
            println!("NSN_OMITIDO: faltan PRONTO_NSN_WAV y PRONTO_NSN_MODELO");
            return;
        };
        let runs: usize = std::env::var("PRONTO_NSN_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
        let samples = crate::audio::decode_wav(&std::fs::read(&wav).expect("leer el WAV")).expect("WAV PCM16 mono 16 kHz");
        let engine = Arc::new(Engine::load_for_tests(&dir).expect("cargar Parakeet"));
        engine.set_vocabulary(&[]);
        let settings = crate::models::Settings::default();
        let opts = crate::text::Options { cleanup: settings.cleanup_level, press_enter: settings.press_enter_command, tone: settings.styles.work, words: &[], snippets: &[] };
        let acabar = |oido: &str| crate::text::process(oido, &opts).text;
        let _ = engine.transcribe_timed(&samples).expect("calentar");

        let (mut entero, mut peor_ms, mut tick_ms) = (Vec::new(), Vec::new(), Vec::new());
        let (mut texto_entero, mut texto_vivo, mut tick_mayor) = (String::new(), String::new(), (0.0, 0.0));
        for _ in 0..runs {
            // Sin texto en vivo.
            let t0 = Instant::now();
            texto_entero = acabar(&crate::state::transcribe_heard(&engine, &[], &samples, false).expect("transcribir").1);
            entero.push(t0.elapsed().as_secs_f64() * 1000.0);
            // Con texto en vivo: lo mismo, con el tick más largo a medias.
            let s = super::tests_dictar_y_soltar(&engine, &samples, PAUSA, &acabar);
            assert!(s.tick_a_medias, "el tick a medias no llegó a transcribir: no es el peor caso");
            texto_vivo = s.texto;
            peor_ms.push(s.ms);
            tick_mayor = (s.tick_mayor_s, s.ticks_ms.iter().copied().fold(0.0, f64::max));
            tick_ms.extend(s.ticks_ms);
        }
        let fmt = |v: &[f64]| v.iter().map(|x| format!("{x:.1}")).collect::<Vec<_>>().join(",");
        println!("NSN_AUDIO_S={:.2}", samples.len() as f64 / RATE as f64);
        println!("NSN_ENTERO_MS={}", fmt(&entero));
        println!("NSN_VIVO_PEOR_MS={}", fmt(&peor_ms));
        println!("NSN_TICK_P50_MS={:.1}", mediana(&tick_ms));
        println!("NSN_TICK_MAYOR={:.1} s de audio, {:.0} ms", tick_mayor.0, tick_mayor.1);
        println!("NSN_TEXTO_ENTERO={texto_entero}");
        println!("NSN_TEXTO_VIVO={texto_vivo}");
        let palabras = |t: &str| t.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).collect::<Vec<_>>();
        let (a, b) = (palabras(&texto_entero), palabras(&texto_vivo));
        let iguales = a.iter().zip(&b).filter(|(x, y)| x == y).count();
        println!("NSN_PALABRAS_IGUALES={iguales}/{}", a.len().max(b.len()));
        assert!(b.len() >= 10, "el texto final con texto en vivo salió casi vacío: {texto_vivo:?}");
    }
}

/// Simulación sin hilos para las pruebas (`tests`, y con el modelo real `tests_dictar_y_soltar`).
#[cfg(test)]
fn tests_simular<D: FnMut(&[f32]) -> Option<Result<Timed, String>>>(audio: &[f32], d: D) -> (Fijado, Vec<Vista>) {
    tests_simular_con(audio, d, PAUSA)
}

/// Como `tests_simular`, con otra pausa (en tramos de 20 ms) para medir.
#[cfg(test)]
fn tests_simular_con<D: FnMut(&[f32]) -> Option<Result<Timed, String>>>(audio: &[f32], d: D, pausa: usize) -> (Fijado, Vec<Vista>) {
    let paso = TICK.as_millis() as usize * RATE / 1000;
    let mut vivo = Vivo::new(d);
    vivo.fijador.pausa = pausa;
    let parar = AtomicBool::new(false);
    let mut vistas = Vec::new();
    let mut t = paso;
    while t <= audio.len() {
        let desde = vivo.fijador.fijado();
        if vivo.tick(&audio[desde..t], &parar) {
            vistas.push(vivo.vista.clone());
        }
        t += paso;
    }
    (vivo.fijado(), vistas)
}

/// Lo que enseña la Flow bar al soltar (la última vista del texto en vivo), para el banco de
/// entendimiento (BELTR-PRT-11).
#[cfg(test)]
pub(crate) fn tests_vista_al_soltar(engine: &Arc<crate::asr::Engine>, words: &[crate::models::Word], samples: &[f32]) -> String {
    let e = engine.clone();
    let (_, vistas) = tests_simular_con(samples, |s: &[f32]| e.transcribe_live(s), PAUSA);
    vistas.last().map(|v| v.con_diccionario(words)).map(|v| format!("{} {}", v.fixed, v.partial).trim().to_string()).unwrap_or_default()
}

/// Un dictado de punta a punta con el modelo real (pruebas): los ticks mientras «se habla» y, al
/// «soltar», el texto final por el camino del dictado (`state::transcribe_heard`, sin diccionario) con
/// el tick más largo recién empezado.
#[cfg(test)]
struct Soltado {
    /// Texto final (`acabar` sobre lo que oyó el dictado).
    texto: String,
    /// De soltar a texto final, en ms.
    ms: f64,
    vistas: Vec<Vista>,
    ticks_ms: Vec<f64>,
    /// Audio (s) del tick más largo, el que va a medias al soltar.
    tick_mayor_s: f64,
    /// ¿Llegó a transcribir ese tick (no se saltó)?
    tick_a_medias: bool,
}

#[cfg(test)]
fn tests_dictar_y_soltar(engine: &Arc<crate::asr::Engine>, samples: &[f32], pausa: usize, acabar: &dyn Fn(&str) -> String) -> Soltado {
    let e = engine.clone();
    let ticks = std::cell::RefCell::new(Vec::new());
    let mayor = std::cell::RefCell::new(Vec::<f32>::new());
    let (_, vistas) = tests_simular_con(
        samples,
        |s: &[f32]| {
            let t = Instant::now();
            let r = e.transcribe_live(s);
            ticks.borrow_mut().push(t.elapsed().as_secs_f64() * 1000.0);
            if s.len() > mayor.borrow().len() {
                *mayor.borrow_mut() = s.to_vec();
            }
            r
        },
        pausa,
    );
    let mayor = mayor.into_inner();
    let tick_mayor_s = mayor.len() as f64 / RATE as f64;
    let (tx, rx) = std::sync::mpsc::channel();
    let e = engine.clone();
    let tick = std::thread::spawn(move || {
        let _ = tx.send(());
        !mayor.is_empty() && e.transcribe_live(&mayor).is_some()
    });
    rx.recv().expect("tick");
    // Que el tick ya esté transcribiendo al soltar.
    std::thread::sleep(Duration::from_millis(20));
    let t0 = Instant::now();
    let texto = acabar(&crate::state::transcribe_heard(engine, &[], samples, false).expect("al soltar").1);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let tick_a_medias = tick.join().expect("tick");
    Soltado { texto, ms, vistas, ticks_ms: ticks.into_inner(), tick_mayor_s, tick_a_medias }
}

/// El texto en vivo no cambia el texto final (BELTR-PRT-10, prueba de Txonso del 19-nov con Whisper:
/// a trozos salía «Va toda hostia esto. Gracias.» donde el audio entero da «Va toda hostia esto.»).
/// La lanza `tests/vivo-whisper.test.mjs` con `PRONTO_VIVO_WHISPER=1` y el modelo large-v3-turbo de
/// `%APPDATA%\Pronto2\models\whisper` (solo lectura). Audio: el de la regla 3 con 1,5 s de silencio y
/// media frase detrás, y los WAV de `PRONTO_VIVO_WAVS` (separados por `;`) si se dan.
#[cfg(test)]
mod vivo_whisper {
    use super::*;
    use crate::asr::Engine;

    #[test]
    fn vivo_whisper_no_inventa_al_soltar() {
        if std::env::var("PRONTO_VIVO_WHISPER").is_err() {
            println!("VIVO_OMITIDO: falta PRONTO_VIVO_WHISPER");
            return;
        }
        let engine = Arc::new(Engine::load_whisper_lang_for_tests("auto").expect("cargar Whisper"));
        // Con palabras del diccionario como pista, que es cuando más inventa en los silencios.
        engine.set_vocabulary(&["BeltranLabs".to_string(), "GitHub".to_string()]);
        let datos = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/no-se-negocia/datos/dictado-15s.wav");
        let base = crate::audio::decode_wav(&std::fs::read(&datos).expect("dictado-15s.wav")).expect("WAV");
        let mut audios = vec![("dictado-15s + silencio + media frase".to_string(), [&base[..], &vec![0.0f32; 24_000], &base[..base.len() / 4]].concat())];
        for w in std::env::var("PRONTO_VIVO_WAVS").unwrap_or_default().split(';').filter(|w| !w.is_empty()) {
            audios.push((w.to_string(), crate::audio::decode_wav(&std::fs::read(w).expect("WAV")).expect("WAV PCM16")));
        }
        let mut fallos = Vec::new();
        for (nombre, s) in &audios {
            let t0 = Instant::now();
            let entero = engine.transcribe_timed(s).expect("entero").text;
            let ms_entero = t0.elapsed().as_secs_f64() * 1000.0;
            let soltado = tests_dictar_y_soltar(&engine, s, PAUSA, &|oido: &str| oido.to_string());
            println!("VIVO_MS=sin texto en vivo {ms_entero:.0} ms · con un tick de {:.1} s a medias {:.0} ms ({})", soltado.tick_mayor_s, soltado.ms, if soltado.tick_a_medias { "transcribía" } else { "se saltó" });
            println!("VIVO_VISTAS={}", soltado.vistas.iter().map(|v| format!("[{}] {}", v.fixed, v.partial)).collect::<Vec<_>>().join(" | "));
            let final_ = soltado.texto;
            println!("VIVO_WAV={nombre}\nVIVO_ENTERO={entero}\nVIVO_FINAL={final_}");
            if final_ != entero {
                fallos.push(format!("{nombre}:\n  entero: {entero}\n  final:  {final_}"));
            }
        }
        assert!(fallos.is_empty(), "con texto en vivo, el texto final del dictado no es el del audio entero sin él:\n{}", fallos.join("\n"));
    }
}

/// Banco con la voz real (BELTR-PRT-10): pasa los WAV de `PRONTO_VIVO_DIR` por el dictado con texto en
/// vivo (ticks reales y, al soltar, el tick más largo a medias) con Parakeet (`PRONTO_NSN_MODELO`) y con
/// Whisper large-v3-turbo (si está su modelo) y compara el texto final con el del audio entero sin texto
/// en vivo. No va en `npm test`: los audios son de Txonso y no entran en el repo.
/// `PRONTO_VIVO_DIR=… PRONTO_NSN_MODELO=… cargo test --release banco_vivo_voz_real -- --ignored --nocapture`
/// (`PRONTO_VIVO_PAUSAS=30,45,60` mide otras pausas, en tramos de 20 ms).
#[cfg(test)]
mod banco_vivo {
    use super::*;
    use crate::asr::Engine;

    #[test]
    #[ignore]
    fn banco_vivo_voz_real() {
        let dir = std::env::var("PRONTO_VIVO_DIR").expect("PRONTO_VIVO_DIR");
        let mut wavs: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        wavs.sort();
        let pausas: Vec<usize> = std::env::var("PRONTO_VIVO_PAUSAS").unwrap_or(PAUSA.to_string()).split(',').map(|p| p.trim().parse().unwrap()).collect();
        let mut motores: Vec<(&str, Arc<Engine>)> = Vec::new();
        if let Ok(m) = std::env::var("PRONTO_NSN_MODELO") {
            motores.push(("parakeet", Arc::new(Engine::load_for_tests(&m).expect("Parakeet"))));
        }
        if std::env::var("PRONTO_VIVO_SIN_WHISPER").is_err() {
            if let Ok(w) = Engine::load_whisper_lang_for_tests("auto") {
                motores.push(("whisper", Arc::new(w)));
            }
        }
        for (nombre, engine) in &motores {
            engine.set_vocabulary(&[]);
            for &pausa in &pausas {
                let (mut iguales, mut t_entero, mut t_final) = (0, 0.0, 0.0);
                for w in &wavs {
                    let s = crate::audio::decode_wav(&std::fs::read(w).unwrap()).unwrap();
                    let t0 = Instant::now();
                    let entero = engine.transcribe_timed(&s).unwrap().text;
                    let ms_entero = t0.elapsed().as_secs_f64() * 1000.0;
                    t_entero += ms_entero;
                    let soltado = tests_dictar_y_soltar(engine, &s, pausa, &|oido: &str| oido.to_string());
                    t_final += soltado.ms;
                    let igual = soltado.texto == entero;
                    iguales += igual as usize;
                    println!(
                        "BANCO {nombre} pausa={}ms {} ({:.1} s) {} · entero {:.0} ms · soltar→texto {:.0} ms (tick a medias: {:.1} s, {})",
                        pausa * 20,
                        w.file_name().unwrap().to_string_lossy(),
                        s.len() as f32 / 16000.0,
                        if igual { "IGUAL" } else { "DISTINTO" },
                        ms_entero,
                        soltado.ms,
                        soltado.tick_mayor_s,
                        if soltado.tick_a_medias { "transcribiendo" } else { "se saltó" },
                    );
                    println!("   entero: {entero}");
                    if !igual {
                        println!("   final:  {}", soltado.texto);
                    }
                }
                println!("BANCO_RESUMEN {nombre} pausa={}ms: {iguales}/{} iguales · soltar→texto total {:.0} ms (entero {:.0} ms)", pausa * 20, wavs.len(), t_final, t_entero);
            }
        }
    }
}
