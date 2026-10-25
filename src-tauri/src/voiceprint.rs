//! Huella de voz del diccionario (Contrato v11). Al grabar la pronunciación de una palabra se guarda
//! también el audio (recortado a la voz). Al dictar se busca en el audio un tramo que SUENE como esas
//! grabaciones y, si casa con claridad, se escribe la palabra en ese tramo del texto aunque Parakeet
//! haya oído otra cosa («Chonzo», «Gihad»…). Va antes de la tubería de texto, sobre el texto crudo.
//!
//! Diseño (medido con los dictados de Txonso y con voz sintética, ver `bateria_voz_real` y
//! `bateria_voz_sintetica`):
//! - Rasgos: MFCC c1..c13 de 40 bandas log-mel (ventana de 25 ms cada 10 ms, FFT propia de 512) más
//!   sus deltas: 26 valores por trama. c0 (la energía) fuera: el volumen no cuenta.
//! - Normalización por enunciado (CMN): a las tramas del dictado Y a las de la muestra se les resta
//!   la media cepstral de la voz del DICTADO. Una palabra suelta tiene su propia media (la de sus
//!   sonidos) y normalizarla con ella la deformaba: con la media del dictado solo se quita el canal
//!   (micro, sala), que es lo que interesa. Varianza sin normalizar (con ella salía peor).
//! - Comparación: DTW de subsecuencia simétrica (Sakoe-Chiba, P=1: pendiente entre 1/2 y 2, coste
//!   normalizado por la longitud de los dos tramos): la muestra entera contra cualquier tramo del
//!   dictado, en una sola pasada por muestra. Se eligió frente a DTW sobre las palabras de Parakeet
//!   porque sus marcas de tiempo bailan ±0,2 s y alargan los finales (la misma palabra no se separaba
//!   de las demás); la asimétrica daba lo mismo y los rasgos log-mel con coseno, peor.
//! - Varias muestras por palabra: se promedia el coste de todas en cada punto (±50 ms de holgura).
//! - Conservador: solo cambia tramos de 1-3 palabras cubiertas al menos a la mitad por el acierto,
//!   que no sean todas corrientes ni otra entrada del diccionario, con coste por debajo del umbral
//!   (`MAX_COST`) y con margen (`MARGIN`) frente a otra palabra del diccionario en el mismo tramo.
//! - Coste: ~35 ms para 50 palabras × 3 muestras y 13 s de audio (repartido en hasta 4 hilos).

use crate::asr::{Timed, TimedToken};
use crate::models::Word;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

const RATE: usize = crate::audio::TARGET_RATE as usize;
/// Ventana de 25 ms cada 10 ms.
const WIN: usize = 400;
const HOP: usize = 160;
const NFFT: usize = 512;
const N_MELS: usize = 40;
/// c1..c13 y sus deltas.
const N_CEPS: usize = 13;
const DIM: usize = N_CEPS * 2;
/// Una muestra útil dura entre 0,2 y 2,5 s de voz.
pub const MIN_SAMPLE_MS: i64 = 200;
const MAX_SAMPLE_MS: i64 = 2500;
/// Holgura (tramas) al juntar el final de las distintas muestras de una palabra.
const END_SLACK: usize = 5;
/// Un acierto cambia como mucho 3 palabras del texto («Bel trán Labs» -> «BeltranLabs»).
const MAX_SPAN_WORDS: usize = 3;
/// Umbral de coste por número de muestras (1, 2, 3 o más). Calibrado con las dos baterías (voz real
/// de Txonso y voz sintética, ver el Contrato v11): el primer falso positivo sale a 6,74 / 7,11 / 6,81
/// con su voz (1 / 2 / 3 muestras) y a 6,56 con la sintética; los aciertos claros quedan entre 3,5 y 6,3.
const MAX_COST: [f32; 3] = [6.3, 6.4, 6.4];
/// Si otra palabra del diccionario casa en el mismo tramo con un coste a menos de esto, no se
/// toca (no está claro cuál es).
const MARGIN: f32 = 0.35;
/// Parte de una palabra del texto que tiene que cubrir el acierto para cambiarla.
const COVER: f32 = 0.5;

// ------------------------------------------------------------------ rasgos

/// FFT compleja radix-2 en sitio (n potencia de 2).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

struct Tables {
    window: Vec<f32>,
    /// Banco mel: por banda, (primer bin, pesos).
    mel: Vec<(usize, Vec<f32>)>,
    /// DCT-II: N_CEPS filas × N_MELS (sin c0).
    dct: Vec<f32>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let window = (0..WIN).map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (WIN - 1) as f32).cos()).collect();
        let hz_to_mel = |f: f32| 2595.0 * (1.0 + f / 700.0).log10();
        let mel_to_hz = |m: f32| 700.0 * (10f32.powf(m / 2595.0) - 1.0);
        let (lo, hi) = (hz_to_mel(60.0), hz_to_mel(7600.0));
        let pts: Vec<f32> = (0..N_MELS + 2).map(|i| mel_to_hz(lo + (hi - lo) * i as f32 / (N_MELS + 1) as f32)).collect();
        let bin_hz = RATE as f32 / NFFT as f32;
        let mel = (0..N_MELS)
            .map(|m| {
                let (l, c, r) = (pts[m], pts[m + 1], pts[m + 2]);
                let first = (l / bin_hz).ceil() as usize;
                let last = ((r / bin_hz).floor() as usize).min(NFFT / 2);
                let w = (first..=last)
                    .map(|b| {
                        let f = b as f32 * bin_hz;
                        if f <= c { (f - l) / (c - l) } else { (r - f) / (r - c) }.max(0.0)
                    })
                    .collect();
                (first, w)
            })
            .collect();
        let mut dct = vec![0.0; N_CEPS * N_MELS];
        for k in 0..N_CEPS {
            for m in 0..N_MELS {
                dct[k * N_MELS + m] = (std::f32::consts::PI * (k + 1) as f32 * (m as f32 + 0.5) / N_MELS as f32).cos();
            }
        }
        Tables { window, mel, dct }
    })
}

/// Cepstros de un audio (sin normalizar): `n` tramas de c1..c13 y la energía (ln) de cada una.
#[derive(Debug, Clone, Default)]
pub struct Ceps {
    pub n: usize,
    data: Vec<f32>,
    energy: Vec<f32>,
}

/// MFCC c1..c13 cada 10 ms (preénfasis 0,97, Hamming, 40 bandas mel de 60 a 7.600 Hz).
pub fn ceps(samples: &[f32]) -> Ceps {
    let t = tables();
    if samples.len() < WIN {
        return Ceps::default();
    }
    let n = 1 + (samples.len() - WIN) / HOP;
    let mut data = Vec::with_capacity(n * N_CEPS);
    let mut energy = Vec::with_capacity(n);
    let (mut re, mut im) = (vec![0.0f32; NFFT], vec![0.0f32; NFFT]);
    let mut logmel = [0.0f32; N_MELS];
    for f in 0..n {
        let s = &samples[f * HOP..f * HOP + WIN];
        re.iter_mut().for_each(|v| *v = 0.0);
        im.iter_mut().for_each(|v| *v = 0.0);
        let mut prev = if f * HOP > 0 { samples[f * HOP - 1] } else { s[0] };
        for i in 0..WIN {
            re[i] = (s[i] - 0.97 * prev) * t.window[i];
            prev = s[i];
        }
        fft(&mut re, &mut im);
        let mut e = 0.0f32;
        for (m, (first, w)) in t.mel.iter().enumerate() {
            let mut acc = 0.0f32;
            for (k, wk) in w.iter().enumerate() {
                let b = first + k;
                acc += wk * (re[b] * re[b] + im[b] * im[b]);
            }
            e += acc;
            logmel[m] = (acc + 1e-8).ln();
        }
        energy.push((e + 1e-8).ln());
        for k in 0..N_CEPS {
            let row = &t.dct[k * N_MELS..(k + 1) * N_MELS];
            data.push(row.iter().zip(&logmel).map(|(a, b)| a * b).sum());
        }
    }
    Ceps { n, data, energy }
}

/// Deltas (pendiente en ± 2 tramas) de los cepstros.
fn deltas(c: &Ceps) -> Vec<f32> {
    let mut out = Vec::with_capacity(c.n * N_CEPS);
    let at = |i: isize, k: usize| c.data[(i.clamp(0, c.n as isize - 1) as usize) * N_CEPS + k];
    for i in 0..c.n as isize {
        for k in 0..N_CEPS {
            out.push((at(i + 1, k) - at(i - 1, k) + 2.0 * (at(i + 2, k) - at(i - 2, k))) / 10.0);
        }
    }
    out
}

/// Media cepstral de las tramas con voz (energía a menos de 30 dB de la más fuerte).
fn voice_mean(c: &Ceps) -> [f32; N_CEPS] {
    let mut mean = [0.0f32; N_CEPS];
    let max = c.energy.iter().copied().fold(f32::MIN, f32::max);
    let mut cnt = 0usize;
    for i in (0..c.n).filter(|&i| c.energy[i] >= max - 7.0) {
        cnt += 1;
        for (k, m) in mean.iter_mut().enumerate() {
            *m += c.data[i * N_CEPS + k];
        }
    }
    mean.iter_mut().for_each(|m| *m /= cnt.max(1) as f32);
    mean
}

/// Tramo de voz de una grabación de pronunciación, en muestras: el bloque con más energía de las
/// tramas a menos de 20 dB del pico y 10 dB por encima del fondo (huecos de menos de 250 ms no
/// lo cortan: oclusivas, la pausa de «Beltrán Labs»), con 50 ms de margen. `None` si no hay voz.
pub fn voice_range(samples: &[f32]) -> Option<(usize, usize)> {
    let c = ceps(samples);
    if c.n < 5 {
        return None;
    }
    let max = c.energy.iter().copied().fold(f32::MIN, f32::max);
    let mut sorted = c.energy.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10];
    let th = (max - 4.6).max(floor + 2.3);
    let voiced: Vec<bool> = c.energy.iter().map(|e| *e >= th).collect();
    // Bloques con voz, uniendo huecos cortos; gana el de más energía.
    let mut blocks: Vec<(usize, usize, f32)> = Vec::new();
    for i in (0..c.n).filter(|&i| voiced[i]) {
        let w = (c.energy[i] - floor).max(0.0);
        match blocks.last_mut() {
            Some(b) if i - b.1 <= 25 => {
                b.1 = i + 1;
                b.2 += w;
            }
            _ => blocks.push((i, i + 1, w)),
        }
    }
    let (a, b, _) = blocks.into_iter().max_by(|x, y| x.2.total_cmp(&y.2))?;
    let from = a.saturating_sub(5) * HOP;
    let to = ((b + 5) * HOP + WIN).min(samples.len());
    Some((from, to))
}

/// Huella de una muestra grabada: cepstros crudos y deltas, listos para centrar con la media del
/// dictado en el que se busca.
#[derive(Debug, Clone)]
pub struct Template {
    n: usize,
    stat: Vec<f32>,
    delta: Vec<f32>,
}

impl Template {
    /// Desde el audio ya recortado (`None` si es demasiado corto o largo para ser una palabra).
    pub fn from_audio(samples: &[f32]) -> Option<Self> {
        let ms = samples.len() as i64 * 1000 / RATE as i64;
        if !(MIN_SAMPLE_MS..=MAX_SAMPLE_MS).contains(&ms) {
            return None;
        }
        let c = ceps(samples);
        Some(Self { n: c.n, delta: deltas(&c), stat: c.data })
    }
}

/// Dictado preparado: rasgos centrados con su media de voz, por dimensión (para vectorizar).
struct Prepared {
    n: usize,
    mean: [f32; N_CEPS],
    /// DIM filas de `n` valores.
    cols: Vec<f32>,
}

fn prepare(c: &Ceps) -> Prepared {
    let mean = voice_mean(c);
    let d = deltas(c);
    let mut cols = vec![0.0f32; DIM * c.n];
    for i in 0..c.n {
        for k in 0..N_CEPS {
            cols[k * c.n + i] = c.data[i * N_CEPS + k] - mean[k];
            cols[(N_CEPS + k) * c.n + i] = d[i * N_CEPS + k];
        }
    }
    Prepared { n: c.n, mean, cols }
}

// ------------------------------------------------------------------ DTW

/// DTW de subsecuencia simétrica (Sakoe-Chiba P=1) de la muestra contra todo el dictado. Para
/// cada trama final, el coste del mejor camino normalizado por (M + N del tramo) y su inicio.
fn curve(q: &Template, x: &Prepared) -> (Vec<f32>, Vec<u32>) {
    let (m, n) = (q.n, x.n);
    const INF: f32 = f32::INFINITY;
    if m == 0 || n == 0 {
        return (vec![INF; n], vec![0; n]);
    }
    // Filas de la muestra centradas con la media del dictado.
    let mut rows = vec![0.0f32; m * DIM];
    for i in 0..m {
        for k in 0..N_CEPS {
            rows[i * DIM + k] = q.stat[i * N_CEPS + k] - x.mean[k];
            rows[i * DIM + N_CEPS + k] = q.delta[i * N_CEPS + k];
        }
    }
    let mut d_prev = vec![0.0f32; n];
    let mut d_cur = vec![0.0f32; n];
    let (mut g2, mut g1, mut g) = (vec![INF; n], vec![INF; n], vec![INF; n]);
    let (mut s2, mut s1, mut s) = (vec![0u32; n], vec![0u32; n], vec![0u32; n]);
    for i in 0..m {
        // Distancia de la trama i de la muestra a todas las del dictado (RMS de las 26 dimensiones).
        d_cur.iter_mut().for_each(|v| *v = 0.0);
        let r = &rows[i * DIM..(i + 1) * DIM];
        for (k, &qk) in r.iter().enumerate() {
            let col = &x.cols[k * n..(k + 1) * n];
            for (acc, &v) in d_cur.iter_mut().zip(col) {
                let t = v - qk;
                *acc += t * t;
            }
        }
        d_cur.iter_mut().for_each(|v| *v = (*v * (1.0 / DIM as f32)).sqrt());
        if i == 0 {
            // Arranque libre en cualquier trama.
            for j in 0..n {
                g[j] = 2.0 * d_cur[j];
                s[j] = j as u32;
            }
        } else {
            for j in 0..n {
                let dc = d_cur[j];
                let mut best = INF;
                let mut st = 0;
                if j >= 1 {
                    // (i-1, j-1) -> (i, j)
                    let a = g1[j - 1] + 2.0 * dc;
                    if a < best {
                        best = a;
                        st = s1[j - 1];
                    }
                    // (i-2, j-1) -> (i-1, j) -> (i, j)
                    let b = g2[j - 1] + 2.0 * d_prev[j] + dc;
                    if b < best {
                        best = b;
                        st = s2[j - 1];
                    }
                }
                if j >= 2 {
                    // (i-1, j-2) -> (i, j-1) -> (i, j)
                    let c = g1[j - 2] + 2.0 * d_cur[j - 1] + dc;
                    if c < best {
                        best = c;
                        st = s1[j - 2];
                    }
                }
                g[j] = best;
                s[j] = st;
            }
        }
        std::mem::swap(&mut g2, &mut g1);
        std::mem::swap(&mut g1, &mut g);
        std::mem::swap(&mut s2, &mut s1);
        std::mem::swap(&mut s1, &mut s);
        std::mem::swap(&mut d_prev, &mut d_cur);
    }
    let cost = (0..n).map(|j| if g1[j].is_finite() { g1[j] / (m + j + 1 - s1[j] as usize) as f32 } else { INF }).collect();
    (cost, s1)
}

/// Mínimo de la curva en ± `END_SLACK` tramas (con el inicio de ese mínimo).
fn slack_min(cost: &[f32], start: &[u32]) -> (Vec<f32>, Vec<u32>) {
    let n = cost.len();
    let mut c = vec![f32::INFINITY; n];
    let mut s = vec![0u32; n];
    for j in 0..n {
        for k in j.saturating_sub(END_SLACK)..(j + END_SLACK + 1).min(n) {
            if cost[k] < c[j] {
                c[j] = cost[k];
                s[j] = start[k];
            }
        }
    }
    (c, s)
}

// ------------------------------------------------------------------ detección

/// Una palabra del diccionario con huella: lo que se escribe, sus formas (para no pisar otras
/// entradas) y sus muestras.
pub struct BankWord {
    /// Se escribe esto (la palabra de la entrada; si tiene reemplazo, la tubería lo aplica luego).
    pub write: String,
    /// La entrada tiene reemplazo.
    pub replaced: bool,
    /// Formas que ya cuentan como esta palabra (palabra, reemplazo, variantes), plegadas.
    pub forms: Vec<String>,
    pub templates: Vec<Arc<Template>>,
}

/// Un tramo del audio que suena como una palabra del diccionario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub word: usize,
    pub cost: f32,
    /// Tramas [start, end) a 10 ms.
    pub start: usize,
    pub end: usize,
}

/// Mínimos locales del coste medio de cada palabra por debajo de `cap`.
pub fn detect(bank: &[BankWord], audio: &[f32], cap: f32) -> Vec<Hit> {
    let c = ceps(audio);
    if c.n == 0 || bank.is_empty() {
        return Vec::new();
    }
    let x = prepare(&c);
    // Una curva por muestra, repartidas entre hilos.
    let jobs: Vec<(usize, &Arc<Template>)> = bank.iter().enumerate().flat_map(|(w, b)| b.templates.iter().map(move |t| (w, t))).collect();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4).min(jobs.len().max(1));
    let per = jobs.len().div_ceil(threads);
    let curves: Vec<(usize, Vec<f32>, Vec<u32>)> = std::thread::scope(|sc| {
        let handles: Vec<_> = jobs
            .chunks(per.max(1))
            .map(|chunk| {
                let x = &x;
                sc.spawn(move || {
                    chunk
                        .iter()
                        .map(|(w, t)| {
                            let (cost, start) = curve(t, x);
                            let (c, s) = slack_min(&cost, &start);
                            (*w, c, s)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    let mut hits = Vec::new();
    for (w, b) in bank.iter().enumerate() {
        let mine: Vec<&(usize, Vec<f32>, Vec<u32>)> = curves.iter().filter(|c| c.0 == w).collect();
        if mine.is_empty() {
            continue;
        }
        let n = x.n;
        let comb: Vec<f32> = (0..n).map(|j| mine.iter().map(|c| c.1[j]).sum::<f32>() / mine.len() as f32).collect();
        let half = (b.templates.iter().map(|t| t.n).min().unwrap_or(1) / 2).max(1);
        for j in 0..n {
            let v = comb[j];
            if !(v < cap) {
                continue;
            }
            let (lo, hi) = (j.saturating_sub(half), (j + half + 1).min(n));
            if comb[lo..j].iter().any(|o| *o <= v) || comb[j + 1..hi].iter().any(|o| *o < v) {
                continue;
            }
            // Inicio: la mediana de los inicios de cada muestra.
            let mut starts: Vec<u32> = mine.iter().map(|c| c.2[j]).collect();
            starts.sort_unstable();
            hits.push(Hit { word: w, cost: v, start: starts[starts.len() / 2] as usize, end: j + 1 });
        }
    }
    hits
}

/// Una palabra del texto de Parakeet: su tramo de tiempo y dónde está en el texto.
#[derive(Debug, Clone, PartialEq)]
pub struct TextWord {
    pub start: f32,
    pub end: f32,
    pub range: std::ops::Range<usize>,
}

/// Palabras a partir de los tokens (un token que empieza por espacio abre palabra; los signos sueltos
/// no cuentan). Devuelve también el texto que forman los tokens.
pub fn text_words(tokens: &[TimedToken]) -> (String, Vec<TextWord>) {
    let mut text = String::new();
    let mut words: Vec<TextWord> = Vec::new();
    let mut open = false;
    for t in tokens {
        let starts_word = t.text.starts_with(' ') || text.is_empty();
        let body = t.text.trim_start();
        let lead = text.len() + (t.text.len() - body.len());
        text.push_str(&t.text);
        // Solo la parte alfanumérica del token (quita signos pegados: «Chonso,»).
        let first = body.char_indices().find(|(_, c)| c.is_alphanumeric()).map(|(i, _)| i);
        let last = body.char_indices().filter(|(_, c)| c.is_alphanumeric()).last().map(|(i, c)| i + c.len_utf8());
        let (Some(a), Some(b)) = (first, last) else {
            open = false;
            continue;
        };
        match words.last_mut() {
            Some(w) if open && !starts_word && w.range.end == lead + a => {
                w.range.end = lead + b;
                w.end = t.end;
            }
            _ => {
                words.push(TextWord { start: t.start, end: t.end, range: lead + a..lead + b });
                open = true;
            }
        }
        if b < body.len() {
            open = false; // signo detrás: la palabra se cierra
        }
    }
    (text, words)
}

/// Lo que se cambió en el texto.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub from: String,
    pub to: String,
    pub cost: f32,
    /// La entrada tiene reemplazo: la tubería pasará `to` a su reemplazo (y contará el arreglo).
    pub replaced: bool,
}

/// Un acierto aceptado: palabra del banco, palabras del texto [first, last] y coste. `noop`: el
/// texto ya dice esa palabra (o una forma que la tubería corrige); ocupa el tramo pero no cambia nada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub word: usize,
    pub first: usize,
    pub last: usize,
    pub cost: f32,
    pub noop: bool,
}

/// Palabras del texto que ocupa un acierto: las que quedan cubiertas al menos en un `COVER` de su
/// duración (una palabra larga de la que solo casa un trozo no cuenta: «trimestral» no es
/// «Claude»), más las vecinas tocadas que son principio o final de la palabra del diccionario
/// («Para Kit» -> «Parakeet», «Beltrán Labs» -> «BeltranLabs»).
fn span_for(h: &Hit, text: &str, words: &[TextWord], write: &str) -> Option<(usize, usize)> {
    let (ts, te) = (h.start as f32 * 0.01, h.end as f32 * 0.01 + 0.015);
    let ov = |w: &TextWord| te.min(w.end) - ts.max(w.start);
    let core: Vec<usize> = (0..words.len()).filter(|&i| ov(&words[i]) >= COVER * (words[i].end - words[i].start)).collect();
    let (mut first, mut last) = (*core.first()?, *core.last()?);
    let target: String = crate::text::fold(write).chars().filter(|c| c.is_alphanumeric()).collect();
    let piece = |i: usize| crate::text::fold(&text[words[i].range.clone()]);
    if first > 0 && ov(&words[first - 1]) > -0.15 {
        let p = piece(first - 1);
        if p.chars().count() >= 2 && target.starts_with(&p) && !target.starts_with(&piece(first)) {
            first -= 1;
        }
    }
    if last + 1 < words.len() && ov(&words[last + 1]) > -0.15 {
        let p = piece(last + 1);
        if p.chars().count() >= 2 && target.ends_with(&p) && !target.ends_with(&piece(last)) {
            last += 1;
        }
    }
    Some((first, last))
}

/// Decide qué aciertos se aplican y sobre qué palabras del texto (conservador, ver el módulo).
/// `limits`: coste máximo con 1, 2 y 3+ muestras (en la app, `MAX_COST`).
pub fn decide(bank: &[BankWord], hits: &[Hit], text: &str, words: &[TextWord], limits: &[f32; 3]) -> Vec<Choice> {
    let mut cands: Vec<Choice> = Vec::new();
    for h in hits {
        let b = &bank[h.word];
        if h.cost >= limits[b.templates.len().clamp(1, 3) - 1] {
            continue;
        }
        let Some((first, last)) = span_for(h, text, words, &b.write) else { continue };
        if last - first + 1 > MAX_SPAN_WORDS {
            continue;
        }
        cands.push(Choice { word: h.word, first, last, cost: h.cost, noop: false });
    }
    // Margen: en un tramo que se solapa con el de otra palabra del diccionario, solo vale el claro.
    let overlaps = |a: &Choice, b: &Choice| a.first <= b.last && b.first <= a.last;
    let mut ok: Vec<Choice> = cands.iter().filter(|c| !cands.iter().any(|o| o.word != c.word && overlaps(c, o) && o.cost < c.cost + MARGIN)).copied().collect();
    ok.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    let mut chosen: Vec<Choice> = Vec::new();
    for mut c in ok {
        // Una palabra que existe en el borde del tramo y no es trozo de la del diccionario no se
        // la lleva («está sana» -> «Asana» se comía «está»; rompedor 23-oct, dictado MEDIA 5). Los
        // trozos sí se juntan: «Para Kit» -> «Parakeet».
        let target: String = crate::text::fold(&bank[c.word].write).chars().filter(|ch| ch.is_alphanumeric()).collect();
        let piece = |i: usize| crate::text::fold(&text[words[i].range.clone()]).chars().filter(|ch| ch.is_alphanumeric()).collect::<String>();
        while c.first < c.last && crate::text::is_real_word(&text[words[c.first].range.clone()]) && !target.starts_with(&piece(c.first)) {
            c.first += 1;
        }
        while c.last > c.first && crate::text::is_real_word(&text[words[c.last].range.clone()]) && !target.ends_with(&piece(c.last)) {
            c.last -= 1;
        }
        if chosen.iter().any(|o| overlaps(&c, o)) {
            continue;
        }
        let found = &text[words[c.first].range.start..words[c.last].range.end];
        let folded = crate::text::fold(&crate::text::normalize_heard(found));
        // Ya es la palabra (o una forma que la tubería corrige), o la palabra con algo detrás
        // («estilos», «Pronto2»): nada que hacer. Las mismas letras con otros espacios o tildes
        // («Beltrán Labs» por «BeltranLabs») sí se escriben como en el diccionario.
        let squash = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
        let sq = squash(&folded);
        let longer = |f: &str| !f.is_empty() && sq.len() > f.len() && sq.starts_with(f);
        if bank[c.word].forms.iter().any(|f| *f == folded || longer(&squash(f))) {
            c.noop = true;
            chosen.push(c);
            continue;
        }
        // Nunca palabras corrientes ni otra entrada del diccionario. Tampoco un tramo de palabras
        // que existen (el léxico de «suena igual»): «también está sana» se pegaba «también Asana»
        // con «Asana» grabada (rompedor 23-oct, dictado MEDIA 5).
        let other_entry = bank.iter().flat_map(|b| &b.forms).any(|f| *f == folded || folded.split(' ').any(|w| w == f.as_str()));
        if crate::text::is_common_phrase(found) || crate::text::is_real_phrase(found) || other_entry {
            continue;
        }
        chosen.push(c);
    }
    chosen
}

/// Cambios de la huella sobre una transcripción como (rango de bytes en el texto de los tokens,
/// palabra nueva), para quien trabaja con los tiempos de cada palabra (Meets, Contrato v12).
pub fn corrections(bank: &[BankWord], audio: &[f32], timed: &Timed) -> Vec<(std::ops::Range<usize>, String)> {
    let (text, words) = text_words(&timed.tokens);
    let same = |a: &str, b: &str| a.split_whitespace().eq(b.split_whitespace());
    if bank.is_empty() || words.is_empty() || !same(&text, &timed.text) {
        return Vec::new();
    }
    let cap = MAX_COST.iter().copied().fold(0.0, f32::max);
    let hits = detect(bank, audio, cap);
    decide(bank, &hits, &text, &words, &MAX_COST)
        .into_iter()
        .filter(|c| !c.noop)
        .map(|c| {
            let range = words[c.first].range.start..words[c.last].range.end;
            let found = &text[range.clone()];
            let mut to = bank[c.word].write.clone();
            if !to.chars().any(char::is_uppercase) && found.chars().next().is_some_and(char::is_uppercase) {
                let mut cs = to.chars();
                to = cs.next().map(|c| c.to_uppercase().chain(cs).collect()).unwrap_or_default();
            }
            (range, to)
        })
        .collect()
}

/// Aplica la huella a una transcripción: devuelve el texto crudo corregido y los cambios.
pub fn correct_with_bank(bank: &[BankWord], audio: &[f32], timed: &Timed) -> (String, Vec<Fix>) {
    let (text, words) = text_words(&timed.tokens);
    // Si los tokens no rehacen el texto (no debería pasar), mejor no tocar nada.
    let same = |a: &str, b: &str| a.split_whitespace().eq(b.split_whitespace());
    if bank.is_empty() || words.is_empty() || !same(&text, &timed.text) {
        return (timed.text.clone(), Vec::new());
    }
    let cap = MAX_COST.iter().copied().fold(0.0, f32::max);
    let hits = detect(bank, audio, cap);
    let mut chosen: Vec<Choice> = decide(bank, &hits, &text, &words, &MAX_COST).into_iter().filter(|c| !c.noop).collect();
    if chosen.is_empty() {
        return (timed.text.clone(), Vec::new());
    }
    // De atrás adelante para no mover los índices.
    chosen.sort_by(|a, b| b.first.cmp(&a.first));
    let mut out = text.clone();
    let mut fixes = Vec::new();
    for c in chosen {
        let range = words[c.first].range.start..words[c.last].range.end;
        let found = out[range.clone()].to_string();
        let mut to = bank[c.word].write.clone();
        // Mayúscula de inicio de frase si la palabra es toda minúsculas.
        if !to.chars().any(char::is_uppercase) && found.chars().next().is_some_and(char::is_uppercase) {
            let mut cs = to.chars();
            to = cs.next().map(|c| c.to_uppercase().chain(cs).collect()).unwrap_or_default();
        }
        out.replace_range(range, &to);
        fixes.push(Fix { from: found, to, cost: c.cost, replaced: bank[c.word].replaced });
    }
    fixes.reverse();
    (out.trim().to_string(), fixes)
}

// ------------------------------------------------------------------ banco desde el diccionario

/// Huellas cargadas, por fichero de muestra (se cargan una vez; ~1 ms por muestra).
fn cache() -> &'static Mutex<HashMap<String, Option<Arc<Template>>>> {
    static C: OnceLock<Mutex<HashMap<String, Option<Arc<Template>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn load_template(file: &str) -> Option<Arc<Template>> {
    if let Some(t) = cache().lock().get(file) {
        return t.clone();
    }
    let t = crate::db::voice_dir()
        .and_then(|d| std::fs::read(d.join(file)).ok())
        .and_then(|b| crate::audio::decode_wav(&b))
        .and_then(|s| Template::from_audio(&s))
        .map(Arc::new);
    if t.is_none() {
        eprintln!("[voz] no se pudo leer la muestra {file}");
    }
    cache().lock().insert(file.to_string(), t.clone());
    t
}

/// Banco con las palabras del diccionario que tienen muestras de voz.
pub fn bank(words: &[Word]) -> Vec<BankWord> {
    let mut out = Vec::new();
    for w in words.iter().filter(|w| !w.samples.is_empty() && !w.word.trim().is_empty()) {
        let templates: Vec<Arc<Template>> = w.samples.iter().filter_map(|s| load_template(&s.file)).collect();
        if templates.is_empty() {
            continue;
        }
        let fold = |s: &str| crate::text::fold(&crate::text::normalize_heard(s));
        let mut forms = vec![fold(&w.word)];
        forms.extend(w.replacement.as_deref().map(fold));
        forms.extend(w.variants.iter().map(|v| fold(&v.heard)));
        forms.retain(|f| !f.is_empty());
        let replaced = w.replacement.as_deref().is_some_and(|r| !r.trim().is_empty());
        out.push(BankWord { write: w.word.trim().to_string(), replaced, forms, templates });
    }
    // Lo que ya no está en el diccionario sale de la caché.
    let live: std::collections::HashSet<&str> = words.iter().flat_map(|w| w.samples.iter().map(|s| s.file.as_str())).collect();
    cache().lock().retain(|k, _| live.contains(k.as_str()));
    out
}

/// ¿La huella oye en `audio` alguna palabra del diccionario con muestras? Barato si no hay muestras.
/// Con Whisper por su camino rápido (tiempos aproximados), solo entonces se repite el dictado con
/// tiempos finos para poder corregir (`state::transcribe_processed_as`).
pub fn hears(words: &[Word], audio: &[f32]) -> bool {
    let b = bank(words);
    !b.is_empty() && !detect(&b, audio, MAX_COST.iter().copied().fold(0.0, f32::max)).is_empty()
}

/// Huella sobre un dictado con el diccionario actual: texto crudo corregido y cambios. Barato si no
/// hay muestras (no calcula nada).
pub fn correct(words: &[Word], audio: &[f32], timed: &Timed) -> (String, Vec<Fix>) {
    let b = bank(words);
    if b.is_empty() {
        return (timed.text.clone(), Vec::new());
    }
    let t0 = std::time::Instant::now();
    let (text, fixes) = correct_with_bank(&b, audio, timed);
    let n: usize = b.iter().map(|w| w.templates.len()).sum();
    eprintln!("[voz] {n} muestras, {} ms de audio: {} ms, {} cambios", audio.len() * 1000 / RATE, t0.elapsed().as_millis(), fixes.len());
    for f in &fixes {
        eprintln!("[voz] «{}» -> «{}» (coste {:.2})", f.from, f.to, f.cost);
    }
    (text, fixes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_de_un_seno() {
        let n = 64;
        let mut re: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * 5.0 * i as f32 / n as f32).cos()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mag: Vec<f32> = re.iter().zip(&im).map(|(a, b)| (a * a + b * b).sqrt()).collect();
        assert!((mag[5] - 32.0).abs() < 1e-3 && (mag[59] - 32.0).abs() < 1e-3, "{mag:?}");
        assert!(mag.iter().enumerate().filter(|(i, _)| *i != 5 && *i != 59).all(|(_, m)| *m < 1e-3));
    }

    /// Generador pseudoaleatorio fijo (las pruebas no dependen de nada externo).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    /// «Sílaba» sintética: dos formantes durante `ms` con algo de ruido. Cada `seed` suena distinto.
    fn syllable(seed: u64, ms: usize, rng: &mut Rng) -> Vec<f32> {
        let f1 = 300.0 + (seed * 137 % 600) as f32;
        let f2 = 900.0 + (seed * 811 % 1800) as f32;
        let n = ms * RATE / 1000;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let env = (std::f32::consts::PI * i as f32 / n as f32).sin();
                env * (0.3 * (2.0 * std::f32::consts::PI * f1 * t).sin() + 0.2 * (2.0 * std::f32::consts::PI * f2 * t).sin()) + 0.01 * rng.next()
            })
            .collect()
    }

    fn word_audio(seeds: &[u64], rng: &mut Rng) -> Vec<f32> {
        seeds.iter().flat_map(|s| syllable(*s, 140, rng)).collect()
    }

    fn silence(ms: usize, rng: &mut Rng) -> Vec<f32> {
        (0..ms * RATE / 1000).map(|_| 0.003 * rng.next()).collect()
    }

    fn tok(text: &str, start: f32, end: f32) -> TimedToken {
        TimedToken { text: text.into(), start, end }
    }

    #[test]
    fn recorta_a_la_voz() {
        let mut rng = Rng(1);
        let mut a = silence(600, &mut rng);
        a.extend(syllable(9, 50, &mut rng)); // clic suelto
        a.extend(silence(700, &mut rng));
        let from = a.len();
        a.extend(word_audio(&[3, 5, 7], &mut rng));
        let to = a.len();
        a.extend(silence(800, &mut rng));
        let (s, e) = voice_range(&a).expect("hay voz");
        assert!(s <= from && from - s < RATE / 10, "inicio {s} frente a {from}");
        assert!(e >= to - RATE / 50 && e - to < RATE / 8, "fin {e} frente a {to}");
        assert!(voice_range(&[0.0; 100]).is_none());
    }

    #[test]
    fn palabras_desde_los_tokens() {
        let toks = [tok(" Le", 0.1, 0.3), tok(" dije", 0.3, 0.6), tok(" a", 0.6, 0.7), tok(" Ch", 0.8, 1.0), tok("onso", 1.0, 1.3), tok(",", 1.3, 1.4), tok(" vale", 1.5, 1.8), tok(".", 1.8, 1.9)];
        let (text, ws) = text_words(&toks);
        assert_eq!(text.trim(), "Le dije a Chonso, vale.");
        let got: Vec<&str> = ws.iter().map(|w| &text[w.range.clone()]).collect();
        assert_eq!(got, ["Le", "dije", "a", "Chonso", "vale"]);
        assert_eq!((ws[3].start, ws[3].end), (0.8, 1.3));
    }

    /// Dictado sintético con la «palabra» en medio y tokens que la transcriben mal.
    fn dictation(word: &[u64], other: &[u64], rng: &mut Rng) -> (Vec<f32>, Timed, f32, f32) {
        let mut a = silence(300, rng);
        a.extend(word_audio(other, rng));
        a.extend(silence(150, rng));
        let ws = a.len() as f32 / RATE as f32;
        a.extend(word_audio(word, rng));
        let we = a.len() as f32 / RATE as f32;
        a.extend(silence(150, rng));
        a.extend(word_audio(&other[..2], rng));
        a.extend(silence(300, rng));
        let toks = vec![tok(" Dile", 0.3, ws - 0.15), tok(" a", ws - 0.15, ws - 0.05), tok(" Ch", ws, ws + 0.2), tok("onzo", ws + 0.2, we), tok(" ya", we + 0.15, we + 0.4), tok(".", we + 0.4, we + 0.45)];
        let text = "Dile a Chonzo ya.".to_string();
        (a, Timed { text, tokens: toks }, ws, we)
    }

    fn bank_of(word: &str, seeds: &[u64], n: usize, rng: &mut Rng) -> BankWord {
        let templates = (0..n).map(|_| Arc::new(Template::from_audio(&word_audio(seeds, rng)).unwrap())).collect();
        BankWord { write: word.into(), replaced: false, forms: vec![crate::text::fold(word)], templates }
    }

    #[test]
    fn encuentra_la_palabra_por_como_suena() {
        let mut rng = Rng(7);
        let txonso = [11, 23, 31];
        let bank = vec![bank_of("Txonso", &txonso, 3, &mut rng)];
        let (audio, timed, ws, we) = dictation(&txonso, &[40, 41, 42, 43], &mut rng);
        let hits = detect(&bank, &audio, 99.0);
        let best = hits.iter().min_by(|a, b| a.cost.total_cmp(&b.cost)).unwrap();
        let (s, e) = (best.start as f32 * 0.01, best.end as f32 * 0.01);
        assert!((s - ws).abs() < 0.08 && (e - we).abs() < 0.08, "tramo {s}-{e} frente a {ws}-{we}");
        let (text, fixes) = correct_with_bank(&bank, &audio, &timed);
        assert_eq!(text, "Dile a Txonso ya.");
        assert_eq!(fixes.len(), 1);
        assert_eq!((fixes[0].from.as_str(), fixes[0].to.as_str()), ("Chonzo", "Txonso"));
    }

    #[test]
    fn no_cambia_lo_que_suena_distinto() {
        let mut rng = Rng(9);
        let bank = vec![bank_of("Txonso", &[11, 23, 31], 3, &mut rng)];
        // Mismo esquema de dictado, pero en el hueco suena otra palabra.
        let (audio, timed, _, _) = dictation(&[50, 61, 77], &[40, 41, 42, 43], &mut rng);
        let (text, fixes) = correct_with_bank(&bank, &audio, &timed);
        assert_eq!(text, timed.text);
        assert!(fixes.is_empty(), "{fixes:?}");
    }

    #[test]
    fn guardas_de_la_decision() {
        let text = " Le dije a Chonzo que ya";
        let toks = [tok(" Le", 0.0, 0.2), tok(" dije", 0.2, 0.5), tok(" a", 0.5, 0.6), tok(" Chonzo", 0.6, 1.0), tok(" que", 1.0, 1.2), tok(" ya", 1.2, 1.4)];
        let (t2, words) = text_words(&toks);
        assert_eq!(t2, text);
        let mk = |w: &str, k: usize| BankWord {
            write: w.into(),
            replaced: false,
            forms: vec![crate::text::fold(w)],
            templates: (0..k).map(|_| Arc::new(Template { n: 40, stat: vec![0.0; 40 * N_CEPS], delta: vec![0.0; 40 * N_CEPS] })).collect(),
        };
        let hit = |word, cost, s: f32, e: f32| Hit { word, cost, start: (s * 100.0) as usize, end: (e * 100.0) as usize };
        let bank = vec![mk("Txonso", 3), mk("Ollama", 1)];
        let low = MAX_COST[0] - 1.0;
        // Claro y por debajo del umbral: se cambia «Chonzo».
        assert_eq!(decide(&bank, &[hit(0, low, 0.6, 1.0)], text, &words, &MAX_COST), vec![Choice { word: 0, first: 3, last: 3, cost: low, noop: false }]);
        // Por encima del umbral de 3 muestras, o de 1 muestra (más estricto): nada.
        assert!(decide(&bank, &[hit(0, MAX_COST[2] + 0.01, 0.6, 1.0)], text, &words, &MAX_COST).is_empty());
        assert!(decide(&bank, &[hit(1, MAX_COST[0] + 0.01, 0.6, 1.0)], text, &words, &MAX_COST).is_empty());
        // Palabras corrientes («que ya»): nunca.
        assert!(decide(&bank, &[hit(0, low, 1.0, 1.4)], text, &words, &MAX_COST).is_empty());
        // Dos palabras del diccionario casi empatadas en el mismo tramo: no se toca; con margen, gana la mejor.
        assert!(decide(&bank, &[hit(0, low, 0.6, 1.0), hit(1, low + 0.1, 0.6, 1.0)], text, &words, &MAX_COST).is_empty());
        assert_eq!(decide(&bank, &[hit(0, low, 0.6, 1.0), hit(1, low + MARGIN + 0.1, 0.6, 1.0)], text, &words, &MAX_COST), vec![Choice { word: 0, first: 3, last: 3, cost: low, noop: false }]);
        // Ya pone la palabra (o una forma suya): nada que hacer.
        let toks2 = [tok(" a", 0.5, 0.6), tok(" Txonso", 0.6, 1.0)];
        let (t3, w3) = text_words(&toks2);
        let d = decide(&bank, &[hit(0, low, 0.6, 1.0)], &t3, &w3, &MAX_COST);
        assert!(d.len() == 1 && d[0].noop, "{d:?}");
        // Tramo de más de 3 palabras: no.
        assert!(decide(&bank, &[hit(0, low, 0.0, 1.0)], text, &words, &MAX_COST).is_empty());
        // Rompedor 23-oct, dictado MEDIA 5: palabras que existen («está sana» con «Asana» grabada)
        // no se cambian, y una palabra real del borde que no es trozo de la entrada no se lleva.
        let toks3 = [tok(" ya", 0.0, 0.3), tok(" está", 0.3, 0.6), tok(" sana", 0.6, 1.0), tok(" Asanna", 1.0, 1.4)];
        let (t4, w4) = text_words(&toks3);
        let asana = vec![mk("Asana", 3)];
        assert!(decide(&asana, &[hit(0, low, 0.3, 1.0)], &t4, &w4, &MAX_COST).is_empty());
        assert_eq!(decide(&asana, &[hit(0, low, 0.6, 1.4)], &t4, &w4, &MAX_COST), vec![Choice { word: 0, first: 3, last: 3, cost: low, noop: false }]);
    }

    // ------------------------------------------------------------ baterías con datos reales

    fn read_wav(path: &std::path::Path) -> Option<Vec<f32>> {
        crate::audio::decode_wav(&std::fs::read(path).ok()?)
    }

    /// Un dictado con sus tokens (de `volcar_marcas_de_tiempo`).
    struct Dict {
        id: i64,
        timed: Timed,
        audio: Vec<f32>,
    }

    fn load_marcas(path: &str, audio_dir: &std::path::Path) -> Vec<Dict> {
        let all: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        all.iter()
            .filter_map(|d| {
                let id = d["id"].as_i64()?;
                let tokens = d["tokens"].as_array()?.iter().map(|t| tok(t[0].as_str().unwrap(), t[1].as_f64().unwrap() as f32, t[2].as_f64().unwrap() as f32)).collect();
                let audio = read_wav(&audio_dir.join(format!("{id}.wav")))?;
                Some(Dict { id, timed: Timed { text: d["text"].as_str()?.to_string(), tokens }, audio })
            })
            .collect()
    }

    /// Muestra recortada de un dictado: la palabra ± 150 ms, recortada a las tramas a menos de 13 dB
    /// de la más fuerte (como una grabación suelta, pero sacada de una frase).
    fn cut_sample(audio: &[f32], start: f32, end: f32) -> Option<Template> {
        let a = (((start - 0.15).max(0.0)) * RATE as f32) as usize;
        let b = (((end + 0.15) * RATE as f32) as usize).min(audio.len());
        let cut = audio.get(a..b)?;
        let c = ceps(cut);
        let max = c.energy.iter().copied().fold(f32::MIN, f32::max);
        let v: Vec<usize> = (0..c.n).filter(|&i| c.energy[i] >= max - 3.0).collect();
        let (f, l) = (v.first()?.saturating_sub(2), (v.last()? + 3).min(c.n));
        if l - f < 25 {
            return None;
        }
        Template::from_audio(&cut[f * HOP..((l - 1) * HOP + WIN).min(cut.len())])
    }

    /// Vuelca tokens y marcas de tiempo de Parakeet de los dictados con audio de una COPIA de la base
    /// (`PRONTO_VOZ_DB`, abierta solo para leer) a `PRONTO_VOZ_OUT` (JSON): la entrada de
    /// `bateria_voz_real`. `PRONTO_VOZ_DB=copia.db PRONTO_VOZ_OUT=marcas.json cargo test --release
    /// volcar_marcas_de_tiempo -- --ignored`
    #[test]
    #[ignore]
    fn volcar_marcas_de_tiempo() {
        let (Ok(db), Ok(out)) = (std::env::var("PRONTO_VOZ_DB"), std::env::var("PRONTO_VOZ_OUT")) else { return };
        let audio = dirs::data_dir().unwrap().join("Pronto2").join("audio");
        let model = dirs::data_dir().unwrap().join("Pronto2").join("models").join("parakeet");
        let engine = crate::asr::Engine::load_for_tests(&model.to_string_lossy()).expect("modelo");
        let conn = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let mut stmt = conn.prepare("SELECT id FROM history WHERE has_audio = 1 ORDER BY id").unwrap();
        let ids: Vec<i64> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        let mut all = Vec::new();
        for id in ids {
            let Some(samples) = read_wav(&audio.join(format!("{id}.wav"))) else { continue };
            let t = engine.transcribe_timed(&samples).unwrap();
            let toks: Vec<_> = t.tokens.iter().map(|k| serde_json::json!([k.text, k.start, k.end])).collect();
            all.push(serde_json::json!({ "id": id, "text": t.text, "tokens": toks }));
        }
        std::fs::write(out, serde_json::to_string(&all).unwrap()).unwrap();
    }

    /// Batería con la voz real de Txonso: para cada palabra que se repite en sus dictados, K
    /// apariciones (de dictados distintos) hacen de «muestras grabadas» y se buscan en todos los demás
    /// dictados con todas las palabras a la vez en el diccionario. Acierto = el tramo elegido contiene
    /// la palabra (también si ya estaba bien escrita: no cambia nada, pero la ha reconocido); falso
    /// positivo = cambiaría otra cosa. Lee los WAV de `%APPDATA%\Pronto2\audio` sin tocarlos y las
    /// marcas de tiempo de `PRONTO_VOZ_MARCAS` (JSON de `volcar_marcas_de_tiempo`, hecho sobre una
    /// COPIA de la base). `PRONTO_VOZ_K` = muestras por palabra (3); `PRONTO_VOZ_OUT` = volcado de
    /// candidatos sin umbral para calibrar.
    /// `PRONTO_VOZ_MARCAS=marcas.json cargo test --release bateria_voz_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bateria_voz_real() {
        let Ok(marcas) = std::env::var("PRONTO_VOZ_MARCAS") else { return };
        let k: usize = std::env::var("PRONTO_VOZ_K").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        let audio_dir = dirs::data_dir().unwrap().join("Pronto2").join("audio");
        let dicts = load_marcas(&marcas, &audio_dir);
        let targets = [
            ("pronto", "Pronto"), ("linkedin", "LinkedIn"), ("instagram", "Instagram"), ("beltran", "Beltrán"), ("github", "GitHub"),
            ("docker", "Docker"), ("interruptor", "interruptor"), ("compactar", "compactar"), ("herramienta", "herramienta"),
            ("colores", "colores"), ("version", "versión"), ("estilo", "estilo"), ("whisper", "Whisper"), ("animaciones", "animaciones"),
        ];
        let fold = |s: &str| crate::text::fold(s);
        let has = |d: &Dict, key: &str| {
            let (text, ws) = text_words(&d.timed.tokens);
            ws.iter().any(|w| fold(&text[w.range.clone()]).starts_with(&key[..key.len().min(5)]))
        };
        // Muestras: (palabra, dictado de origen, huella).
        let mut samples: Vec<(usize, i64, Arc<Template>)> = Vec::new();
        for (ti, (key, _)) in targets.iter().enumerate() {
            let mut got = 0;
            for d in &dicts {
                if got == k {
                    break;
                }
                let (text, ws) = text_words(&d.timed.tokens);
                let Some(w) = ws.iter().find(|w| fold(&text[w.range.clone()]) == *key) else { continue };
                if let Some(t) = cut_sample(&d.audio, w.start, w.end) {
                    samples.push((ti, d.id, Arc::new(t)));
                    got += 1;
                }
            }
        }
        let mut rows = Vec::new();
        let (mut tp, mut fp, mut pos) = (vec![0usize; targets.len()], vec![0usize; targets.len()], vec![0usize; targets.len()]);
        let mut fixes_shown = Vec::new();
        let (mut total_ms, mut worst_ms, mut audio_s) = (0f64, 0f64, 0f64);
        for d in &dicts {
            // El diccionario para este dictado: las muestras que no salen de él.
            let mut idx = Vec::new();
            let mut bank: Vec<BankWord> = Vec::new();
            for (ti, (key, write)) in targets.iter().enumerate() {
                let templates: Vec<Arc<Template>> = samples.iter().filter(|s| s.0 == ti && s.1 != d.id).map(|s| s.2.clone()).collect();
                if templates.is_empty() {
                    continue;
                }
                idx.push(ti);
                bank.push(BankWord { write: write.to_string(), replaced: false, forms: vec![key.to_string()], templates });
                if has(d, key) {
                    pos[ti] += 1;
                }
            }
            let (text, words) = text_words(&d.timed.tokens);
            if words.is_empty() {
                continue;
            }
            let t0 = std::time::Instant::now();
            let hits = detect(&bank, &d.audio, 99.0);
            let chosen = decide(&bank, &hits, &text, &words, &MAX_COST);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            total_ms += ms;
            worst_ms = worst_ms.max(ms);
            audio_s += d.audio.len() as f64 / RATE as f64;
            let label = |c: &Choice| {
                let key = targets[idx[c.word]].0;
                (c.first..=c.last).any(|i| fold(&text[words[i].range.clone()]).starts_with(&key[..key.len().min(5)]))
            };
            for c in &chosen {
                let ti = idx[c.word];
                if label(c) {
                    tp[ti] += 1;
                } else {
                    fp[ti] += 1;
                    fixes_shown.push(format!("  FP {} en #{}: «{}» (coste {:.2})", targets[ti].1, d.id, &text[words[c.first].range.start..words[c.last].range.end], c.cost));
                }
            }
            // Para calibrar: todo lo que pasaría las guardas sin umbral.
            if std::env::var("PRONTO_VOZ_OUT").is_ok() {
                for c in decide(&bank, &hits, &text, &words, &[99.0; 3]) {
                    rows.push(serde_json::json!({ "dict": d.id, "word": targets[idx[c.word]].0, "k": bank[c.word].templates.len(),
                        "cost": c.cost, "ok": label(&c), "noop": c.noop, "text": &text[words[c.first].range.start..words[c.last].range.end] }));
                }
            }
        }
        eprintln!("\nBatería de voz real: {} dictados, {:.0} s de audio, {} muestras por palabra", dicts.len(), audio_s, k);
        eprintln!("{:<12} {:>9} {:>6} {:>6}", "palabra", "dictados", "TP", "FP");
        for (ti, (_, write)) in targets.iter().enumerate() {
            eprintln!("{:<12} {:>9} {:>6} {:>6}", write, pos[ti], tp[ti], fp[ti]);
        }
        let (stp, sfp, spos): (usize, usize, usize) = (tp.iter().sum(), fp.iter().sum(), pos.iter().sum());
        eprintln!("TOTAL: recall {stp}/{spos} = {:.1} %, precisión {stp}/{} = {:.1} %", 100.0 * stp as f64 / spos.max(1) as f64, stp + sfp, 100.0 * stp as f64 / (stp + sfp).max(1) as f64);
        fixes_shown.iter().for_each(|l| eprintln!("{l}"));
        eprintln!("Tiempo: {:.1} ms de media por dictado ({:.2} ms por s de audio), peor {:.1} ms", total_ms / dicts.len() as f64, total_ms / audio_s, worst_ms);
        if let Ok(out) = std::env::var("PRONTO_VOZ_OUT") {
            std::fs::write(out, serde_json::to_string(&rows).unwrap()).unwrap();
        }
    }

    /// Coste con un diccionario grande: 50 palabras × 3 muestras contra un dictado de 13 s (la media
    /// de los de Txonso). `cargo test --release coste_50_palabras -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn coste_50_palabras() {
        let mut rng = Rng(3);
        let bank: Vec<BankWord> = (0..50u64).map(|w| bank_of(&format!("palabra{w}"), &[w * 3 + 1, w * 5 + 2, w * 7 + 3, w + 9], 3, &mut rng)).collect();
        let mut audio = Vec::new();
        while audio.len() < 13 * RATE {
            let s = (audio.len() as u64 * 31) % 97;
            audio.extend(word_audio(&[s, s + 1, s + 2], &mut rng));
            audio.extend(silence(120, &mut rng));
        }
        let timed = Timed { text: "algo".into(), tokens: vec![tok(" algo", 0.0, 13.0)] };
        let _ = correct_with_bank(&bank, &audio, &timed); // calentamiento
        let mut times = Vec::new();
        for _ in 0..5 {
            let t0 = std::time::Instant::now();
            let _ = correct_with_bank(&bank, &audio, &timed);
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(|a, b| a.total_cmp(b));
        let t0 = std::time::Instant::now();
        let _ = ceps(&audio);
        eprintln!("50 palabras × 3 muestras, 13 s de audio: mediana {:.1} ms (mín {:.1}, máx {:.1}); solo rasgos {:.1} ms",
            times[2], times[0], times[4], t0.elapsed().as_secs_f64() * 1000.0);
    }

    /// Batería sintética (voz SAPI «Helena», generada a fichero con System.Speech): en
    /// `PRONTO_VOZ_SINT` hay, por palabra, 3 grabaciones sueltas `m_{palabra}_{velocidad}.wav` (como las
    /// del hub, se recortan con `voice_range`), 3 frases que la contienen `f_{palabra}_{n}.wav` y 20
    /// frases sin ninguna `n_*.wav`. Transcribe con el motor real y aplica la huella con las 8
    /// palabras a la vez. `PRONTO_VOZ_K=1` usa solo la grabación a velocidad normal.
    /// `PRONTO_VOZ_SINT=<carpeta> cargo test --release bateria_voz_sintetica -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bateria_voz_sintetica() {
        let Ok(dir) = std::env::var("PRONTO_VOZ_SINT") else { return };
        let dir = std::path::PathBuf::from(dir);
        let k: usize = std::env::var("PRONTO_VOZ_K").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
        let model = dirs::data_dir().unwrap().join("Pronto2").join("models").join("parakeet");
        let engine = crate::asr::Engine::load_for_tests(&model.to_string_lossy()).expect("modelo de Pronto 2");
        let words = ["Txonso", "Ollama", "BeltranLabs", "Kubernetes", "Parakeet", "GitHub", "Claude", "Whisper"];
        let rates: &[i32] = if k == 1 { &[0] } else { &[-2, 0, 2] };
        let bank: Vec<BankWord> = words
            .iter()
            .map(|w| {
                let templates = rates
                    .iter()
                    .filter_map(|r| {
                        let a = read_wav(&dir.join(format!("m_{w}_{r}.wav")))?;
                        let (s, e) = voice_range(&a)?;
                        Template::from_audio(&a[s..e]).map(Arc::new)
                    })
                    .collect();
                BankWord { write: w.to_string(), replaced: false, forms: vec![crate::text::fold(w)], templates }
            })
            .collect();
        let mut files: Vec<(Option<usize>, std::path::PathBuf)> = Vec::new();
        for (wi, w) in words.iter().enumerate() {
            for i in 0..3 {
                files.push((Some(wi), dir.join(format!("f_{w}_{i}.wav"))));
            }
        }
        for i in 0..20 {
            files.push((None, dir.join(format!("n_{i:02}.wav"))));
        }
        let (mut tp, mut fixed, mut fp, mut pos) = (0, 0, 0, 0);
        let mut total_ms = 0.0;
        for (want, path) in &files {
            let audio = read_wav(path).unwrap();
            let timed = engine.transcribe_timed(&audio).unwrap();
            let (text, tw) = text_words(&timed.tokens);
            let t0 = std::time::Instant::now();
            let hits = detect(&bank, &audio, 99.0);
            let chosen = decide(&bank, &hits, &text, &tw, &MAX_COST);
            let (out, _) = correct_with_bank(&bank, &audio, &timed);
            total_ms += t0.elapsed().as_secs_f64() * 1000.0;
            pos += want.is_some() as usize;
            let mut found = false;
            for c in &chosen {
                if Some(c.word) == *want && !found {
                    found = true;
                    tp += 1;
                    fixed += !c.noop as usize;
                } else {
                    fp += 1;
                }
            }
            let name = path.file_name().unwrap().to_string_lossy();
            let marks: Vec<String> = chosen.iter().map(|c| format!("{}{} {:.2}", words[c.word], if c.noop { " (ya bien)" } else { "" }, c.cost)).collect();
            eprintln!("{name:<22} «{}» -> «{out}»  [{}]", timed.text, marks.join(", "));
        }
        eprintln!("\nBatería sintética, {k} muestra(s) por palabra: recall {tp}/{pos} ({fixed} corregidas, el resto ya bien), falsos positivos {fp} en {} frases; {:.1} ms de media", files.len(), total_ms / files.len() as f64);
    }
}
