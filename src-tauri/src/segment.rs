//! Segmentador por energía (VAD simple) para el audio de una fuente de la reunión.
//!
//! Trabaja con audio mono a 16 kHz en tramos de 20 ms. El umbral se adapta al ruido de fondo
//! (media móvil de la energía de los tramos sin voz), la voz se abre tras 40 ms por encima del
//! umbral y se cierra tras un silencio. Dos ajustes (`SegConfig`): `LIVE` para la vista previa en
//! vivo (cierra a los 700 ms, trozos de ≤ 20 s) y `FINAL` para la fase 2 (cierra a los 1,2 s,
//! trozos de ≤ 30 s: una intervención entera). Si un trozo llega al máximo se corta por el tramo
//! más silencioso de sus últimos 3 s, no a mitad de palabra. Es una función pura de las muestras
//! recibidas, así que se prueba con señales sintéticas.

pub const RATE: usize = 16_000;
/// Tramo de análisis: 20 ms.
const FRAME: usize = RATE / 50;
/// Tramos seguidos por encima del umbral para abrir un segmento.
const OPEN_FRAMES: usize = 2;
/// Audio previo que se conserva al abrir (para no comerse el arranque de la palabra).
const PRE_ROLL_FRAMES: usize = 10; // 200 ms
/// Cola de silencio que se conserva al cerrar.
const TAIL_FRAMES: usize = 10;
/// Ventana (al final de un trozo que llega al máximo) donde se busca el punto de corte.
const CUT_SEARCH_FRAMES: usize = 150; // 3 s

/// Cuándo se cierra un segmento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegConfig {
    /// Silencio (en tramos de 20 ms) que cierra un segmento.
    pub close_frames: usize,
    /// Duración máxima de un segmento (en tramos).
    pub max_frames: usize,
}

/// Vista previa en vivo: respuesta rápida, trozos cortos.
pub const LIVE: SegConfig = SegConfig { close_frames: 35, max_frames: 1000 }; // 700 ms, 20 s
/// Transcripción final (fase 2): las pausas dentro de una frase no la parten.
pub const FINAL: SegConfig = SegConfig { close_frames: 60, max_frames: 1500 }; // 1,2 s, 30 s
/// Mínimo de tramos con voz para que el segmento valga la pena (descarta chasquidos).
const MIN_VOICED_FRAMES: usize = 8; // 160 ms
/// Tramos de calibración inicial del ruido de fondo (300 ms).
const CALIBRATION_FRAMES: usize = 15;
/// Nivel a partir del cual hay voz sin esperar a calibrar.
const LOUD_SPEECH: f32 = 0.03;
/// Suelo del umbral: por debajo de esto nunca se considera voz.
const MIN_THRESHOLD: f32 = 0.004;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSegment {
    /// Milisegundos desde el origen de la línea de tiempo del segmentador.
    pub start_ms: i64,
    pub end_ms: i64,
    pub samples: Vec<f32>,
    /// RMS de los tramos con voz (sin el pre-roll ni la cola de silencio).
    pub voiced_rms: f32,
    /// Umbral de apertura vigente al cerrar: sirve para saber si el segmento sobresale del ruido.
    pub threshold: f32,
}

/// Margen sobre el umbral adaptativo para que un segmento del micrófono se tome en serio.
const WEAK_MARGIN: f32 = 1.5;

impl AudioSegment {
    /// Voz apenas por encima del ruido de fondo (un chasquido, un soplido del micro): en el
    /// micrófono casi siempre es basura que Parakeet convierte en «S.» o «Mm.».
    pub fn is_weak(&self) -> bool {
        self.voiced_rms < self.threshold * WEAK_MARGIN
    }
}

pub struct Segmenter {
    cfg: SegConfig,
    /// Muestras recibidas hasta ahora (la línea de tiempo).
    pos: u64,
    pending: Vec<f32>,
    noise: f32,
    /// Tramos analizados (los primeros sirven para calibrar el ruido de fondo).
    frames_seen: usize,
    in_speech: bool,
    loud_run: usize,
    quiet_run: usize,
    voiced: usize,
    /// Suma de la energía (RMS²) de los tramos con voz.
    voiced_sq: f32,
    /// Tramos recientes sin voz (pre-roll mientras no hay segmento abierto).
    recent: std::collections::VecDeque<Vec<f32>>,
    cur: Vec<f32>,
    cur_start: u64,
}

impl Default for Segmenter {
    fn default() -> Self {
        Self::new(0)
    }
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

impl Segmenter {
    /// `start_ms`: instante (en la línea de tiempo de la reunión) al que corresponde la primera muestra.
    pub fn new(start_ms: i64) -> Self {
        Self::with_config(start_ms, LIVE)
    }

    pub fn with_config(start_ms: i64, cfg: SegConfig) -> Self {
        Self {
            cfg,
            pos: (start_ms.max(0) as u64) * RATE as u64 / 1000,
            pending: Vec::new(),
            noise: 0.002,
            frames_seen: 0,
            in_speech: false,
            loud_run: 0,
            quiet_run: 0,
            voiced: 0,
            voiced_sq: 0.0,
            recent: Default::default(),
            cur: Vec::new(),
            cur_start: 0,
        }
    }

    /// Instante actual de la línea de tiempo, en ms.
    #[cfg(test)]
    pub fn now_ms(&self) -> i64 {
        ((self.pos + self.pending.len() as u64) * 1000 / RATE as u64) as i64
    }

    fn on_threshold(&self) -> f32 {
        (self.noise * 3.5).max(MIN_THRESHOLD)
    }

    fn off_threshold(&self) -> f32 {
        (self.noise * 2.0).max(MIN_THRESHOLD * 0.6)
    }

    fn ms(samples: u64) -> i64 {
        (samples * 1000 / RATE as u64) as i64
    }

    /// Añade audio y devuelve los segmentos que se hayan cerrado.
    pub fn push(&mut self, samples: &[f32]) -> Vec<AudioSegment> {
        self.pending.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            let frame_start = self.pos;
            self.pos += FRAME as u64;
            if let Some(seg) = self.process_frame(frame, frame_start) {
                out.push(seg);
            }
        }
        out
    }

    fn process_frame(&mut self, frame: Vec<f32>, frame_start: u64) -> Option<AudioSegment> {
        let level = rms(&frame);
        self.frames_seen += 1;
        if !self.in_speech {
            // Calibración: en los primeros 300 ms el ruido se aprende deprisa y no se abre ningún
            // segmento (salvo voz clarísima), para que un fondo ruidoso no parezca una frase.
            if self.frames_seen <= CALIBRATION_FRAMES && level <= LOUD_SPEECH {
                self.noise = (0.7 * self.noise + 0.3 * level).clamp(0.0003, 0.05);
                self.recent.push_back(frame);
                if self.recent.len() > PRE_ROLL_FRAMES + OPEN_FRAMES {
                    self.recent.pop_front();
                }
                return None;
            }
            if level > self.on_threshold() {
                self.loud_run += 1;
            } else {
                self.loud_run = 0;
                // Solo el silencio enseña cuál es el ruido de fondo.
                self.noise = (0.97 * self.noise + 0.03 * level).clamp(0.0003, 0.05);
            }
            self.recent.push_back(frame);
            if self.recent.len() > PRE_ROLL_FRAMES + OPEN_FRAMES {
                self.recent.pop_front();
            }
            if self.loud_run >= OPEN_FRAMES {
                // Se abre: pre-roll + los tramos ruidosos ya vistos.
                self.in_speech = true;
                self.quiet_run = 0;
                self.voiced = self.loud_run;
                self.cur.clear();
                let frames: Vec<Vec<f32>> = self.recent.drain(..).collect();
                self.voiced_sq = frames.iter().rev().take(self.loud_run).map(|f| rms(f).powi(2)).sum();
                self.cur_start = frame_start + FRAME as u64 - (frames.len() * FRAME) as u64;
                for f in frames {
                    self.cur.extend_from_slice(&f);
                }
            }
            return None;
        }

        // En voz.
        if level > self.off_threshold() {
            self.quiet_run = 0;
            self.voiced += 1;
            self.voiced_sq += level * level;
        } else {
            self.quiet_run += 1;
        }
        self.cur.extend_from_slice(&frame);
        let cur_frames = self.cur.len() / FRAME;
        if self.quiet_run >= self.cfg.close_frames {
            return self.close(self.quiet_run);
        }
        if cur_frames >= self.cfg.max_frames {
            return self.cut_long();
        }
        None
    }

    /// Un segmento llegó al máximo hablando: sale hasta el tramo más silencioso de sus últimos 3 s
    /// (en empate, el más tardío) y lo que queda detrás sigue abierto como segmento nuevo.
    fn cut_long(&mut self) -> Option<AudioSegment> {
        let frames = self.cur.len() / FRAME;
        let from = frames.saturating_sub(CUT_SEARCH_FRAMES).max(frames / 2);
        let (mut cut, mut best) = (frames, f32::MAX);
        for i in from..frames {
            let e = rms(&self.cur[i * FRAME..(i + 1) * FRAME]);
            if e <= best {
                best = e;
                cut = i + 1;
            }
        }
        let rest = self.cur.split_off(cut * FRAME);
        // La energía de voz del resto pasa al segmento que sigue abierto.
        let off = self.off_threshold();
        let (rest_voiced, rest_sq) = rest.chunks(FRAME).map(rms).filter(|&l| l > off).fold((0usize, 0f32), |(n, s), l| (n + 1, s + l * l));
        let voiced = self.voiced.saturating_sub(rest_voiced);
        let voiced_sq = (self.voiced_sq - rest_sq).max(0.0);
        let samples = std::mem::replace(&mut self.cur, rest);
        let start = self.cur_start;
        self.cur_start += samples.len() as u64;
        self.voiced = rest_voiced;
        self.voiced_sq = rest_sq;
        self.quiet_run = self.quiet_run.min(self.cur.len() / FRAME);
        if voiced < MIN_VOICED_FRAMES {
            return None;
        }
        let threshold = self.on_threshold();
        Some(AudioSegment { start_ms: Self::ms(start), end_ms: Self::ms(start + samples.len() as u64), samples, voiced_rms: (voiced_sq / voiced.max(1) as f32).sqrt(), threshold })
    }

    /// Cierra el segmento abierto recortando el exceso de silencio final.
    fn close(&mut self, quiet_frames: usize) -> Option<AudioSegment> {
        self.in_speech = false;
        self.loud_run = 0;
        self.quiet_run = 0;
        self.recent.clear();
        let voiced = std::mem::take(&mut self.voiced);
        let voiced_sq = std::mem::take(&mut self.voiced_sq);
        let threshold = self.on_threshold();
        let mut samples = std::mem::take(&mut self.cur);
        if voiced < MIN_VOICED_FRAMES {
            return None;
        }
        let trim = quiet_frames.saturating_sub(TAIL_FRAMES) * FRAME;
        samples.truncate(samples.len().saturating_sub(trim));
        let start = self.cur_start;
        Some(AudioSegment { start_ms: Self::ms(start), end_ms: Self::ms(start + samples.len() as u64), samples, voiced_rms: (voiced_sq / voiced.max(1) as f32).sqrt(), threshold })
    }

    /// Termina la grabación: devuelve lo que quedara abierto.
    pub fn flush(&mut self) -> Option<AudioSegment> {
        if !self.pending.is_empty() && self.in_speech {
            let rest = std::mem::take(&mut self.pending);
            self.pos += rest.len() as u64;
            self.cur.extend_from_slice(&rest);
        }
        if self.in_speech { self.close(self.quiet_run) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tono de `ms` milisegundos con amplitud `amp`.
    fn tone(ms: usize, amp: f32) -> Vec<f32> {
        (0..ms * RATE / 1000).map(|i| (i as f32 * 0.13).sin() * amp).collect()
    }

    /// «Ruido de fondo» determinista y flojo.
    fn hiss(ms: usize, amp: f32) -> Vec<f32> {
        let mut x: u32 = 12345;
        (0..ms * RATE / 1000)
            .map(|_| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                ((x >> 16) as f32 / 32768.0 - 1.0) * amp
            })
            .collect()
    }

    fn run(parts: &[Vec<f32>]) -> Vec<AudioSegment> {
        let mut s = Segmenter::new(0);
        let mut out = Vec::new();
        for p in parts {
            // En trozos irregulares, como llegan del micrófono.
            for chunk in p.chunks(777) {
                out.extend(s.push(chunk));
            }
        }
        out.extend(s.flush());
        out
    }

    #[test]
    fn dos_frases_separadas_por_silencio() {
        let segs = run(&[hiss(1000, 0.001), tone(1500, 0.2), hiss(1200, 0.001), tone(500, 0.2), hiss(2000, 0.001)]);
        assert_eq!(segs.len(), 2, "{:?}", segs.iter().map(|s| (s.start_ms, s.end_ms)).collect::<Vec<_>>());
        // Primera: voz de 1000 a 2500 ms (con pre-roll y cola).
        assert!((segs[0].start_ms - 800).abs() <= 100, "{}", segs[0].start_ms);
        assert!(segs[0].end_ms >= 2500 && segs[0].end_ms <= 2800, "{}", segs[0].end_ms);
        // Segunda: voz de 3700 a 4200 ms.
        assert!((segs[1].start_ms - 3500).abs() <= 100, "{}", segs[1].start_ms);
        assert!(segs[1].end_ms >= 4200 && segs[1].end_ms <= 4500, "{}", segs[1].end_ms);
        assert!(segs[0].end_ms < segs[1].start_ms);
        // Las muestras casan con la duración.
        let s = &segs[0];
        assert_eq!(s.samples.len() as i64 * 1000 / RATE as i64, s.end_ms - s.start_ms);
    }

    #[test]
    fn una_pausa_corta_no_parte_la_frase() {
        // 400 ms de pausa < 700 ms: sigue siendo un solo segmento.
        let segs = run(&[hiss(500, 0.001), tone(800, 0.2), hiss(400, 0.001), tone(800, 0.2), hiss(1500, 0.001)]);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].end_ms - segs[0].start_ms > 2000);
    }

    #[test]
    fn el_silencio_y_los_chasquidos_no_generan_segmentos() {
        assert!(run(&[hiss(5000, 0.001)]).is_empty());
        assert!(run(&[hiss(500, 0.001), tone(60, 0.3), hiss(2000, 0.001)]).is_empty());
        assert!(run(&[vec![0.0; RATE * 3]]).is_empty());
    }

    #[test]
    fn maximo_de_20_segundos() {
        let segs = run(&[hiss(500, 0.001), tone(45_000, 0.2), hiss(1500, 0.001)]);
        assert_eq!(segs.len(), 3, "{:?}", segs.iter().map(|s| (s.start_ms, s.end_ms)).collect::<Vec<_>>());
        assert!(segs.iter().all(|s| s.end_ms - s.start_ms <= 20_000 + 20));
        // Sin huecos grandes entre trozos consecutivos de una misma intervención.
        assert!((segs[1].start_ms - segs[0].end_ms).abs() <= 40);
    }

    #[test]
    fn el_corte_por_maximo_cae_en_la_pausa() {
        // 25 s hablando con un respiro de 100 ms a los 18,5 s: el corte va ahí, no a los 20 s.
        let segs = run(&[hiss(500, 0.001), tone(18_000, 0.2), hiss(100, 0.001), tone(7_000, 0.2), hiss(1500, 0.001)]);
        assert_eq!(segs.len(), 2, "{:?}", segs.iter().map(|s| (s.start_ms, s.end_ms)).collect::<Vec<_>>());
        assert!((segs[0].end_ms - 18_560).abs() <= 80, "{}", segs[0].end_ms);
        assert_eq!(segs[1].start_ms, segs[0].end_ms);
        assert!(!segs[0].is_weak() && !segs[1].is_weak());
    }

    #[test]
    fn la_fase_final_no_parte_por_pausas_de_un_segundo() {
        let parts = [hiss(500, 0.001), tone(2000, 0.2), hiss(900, 0.001), tone(2000, 0.2), hiss(2500, 0.001)];
        assert_eq!(run(&parts).len(), 2);
        let mut s = Segmenter::with_config(0, FINAL);
        let mut out = Vec::new();
        for p in &parts {
            out.extend(s.push(p));
        }
        out.extend(s.flush());
        assert_eq!(out.len(), 1);
        assert!(out[0].end_ms - out[0].start_ms > 4800);
    }

    #[test]
    fn el_umbral_se_adapta_al_ruido_de_fondo() {
        // Con mucho ruido de fondo (0.02), una voz de 0.2 sigue detectándose y el ruido solo no.
        assert!(run(&[hiss(4000, 0.02)]).is_empty());
        let segs = run(&[hiss(3000, 0.02), tone(1000, 0.2), hiss(2000, 0.02)]);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn lo_que_queda_abierto_sale_al_terminar() {
        let mut s = Segmenter::new(0);
        assert!(s.push(&hiss(500, 0.001)).is_empty());
        assert!(s.push(&tone(1000, 0.2)).is_empty());
        let last = s.flush().expect("segmento abierto");
        assert!(last.end_ms - last.start_ms >= 900);
        assert!(s.flush().is_none());
    }

    #[test]
    fn la_voz_debil_se_marca_y_la_fuerte_no() {
        // Un soplido del micro: apenas sobre el suelo del umbral (0,004) pero lo bastante para abrir.
        let weak = run(&[hiss(500, 0.001), tone(400, 0.0065), hiss(1500, 0.001)]);
        assert_eq!(weak.len(), 1);
        assert!(weak[0].is_weak(), "rms {} umbral {}", weak[0].voiced_rms, weak[0].threshold);
        // Habla normal.
        let strong = run(&[hiss(500, 0.001), tone(800, 0.2), hiss(1500, 0.001)]);
        assert_eq!(strong.len(), 1);
        assert!(!strong[0].is_weak(), "rms {} umbral {}", strong[0].voiced_rms, strong[0].threshold);
        // Con mucho ruido de fondo el umbral sube y la voz débil sigue siéndolo.
        let noisy = run(&[hiss(3000, 0.02), tone(500, 0.07), hiss(1500, 0.02)]);
        assert!(noisy.iter().all(|s| s.is_weak()));
    }

    #[test]
    fn el_origen_se_desplaza() {
        let mut s = Segmenter::new(10_000);
        let mut out = s.push(&hiss(500, 0.001));
        out.extend(s.push(&tone(800, 0.2)));
        out.extend(s.push(&hiss(1200, 0.001)));
        out.extend(s.flush());
        assert_eq!(out.len(), 1);
        assert!(out[0].start_ms >= 10_300 && out[0].start_ms <= 10_600, "{}", out[0].start_ms);
        assert!(s.now_ms() >= 12_400);
    }
}
