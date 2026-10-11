//! Captura de micrófono con `cpal`: mono f32, remuestreo a 16 kHz y nivel RMS en vivo.
//!
//! El `Stream` de cpal no es `Send` en todas las plataformas, así que vive entero en un hilo
//! propio; el resto de la app solo habla con él por canales.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use crossbeam_channel::{bounded, Sender};
use parking_lot::Mutex;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const TARGET_RATE: u32 = 16_000;
/// Tope de seguridad por si el modo manos libres se queda abierto (20 minutos, como Wispr).
const MAX_SECONDS: usize = 1200;
/// Intervalo entre mediciones de nivel (~30 Hz).
const LEVEL_EVERY: Duration = Duration::from_millis(33);

pub type LevelFn = Arc<dyn Fn(f32) + Send + Sync>;

/// Nombres de los micrófonos de entrada disponibles.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = match host.input_devices() {
        Ok(devs) => devs.filter_map(|d| d.description().ok().map(|desc| desc.name().to_string())).collect(),
        Err(e) => {
            eprintln!("[audio] no se pudieron listar los micrófonos: {e}");
            Vec::new()
        }
    };
    names.sort();
    names.dedup();
    names
}

pub(crate) fn pick_device(host: &cpal::Host, name: Option<&str>) -> Option<cpal::Device> {
    if let Some(name) = name {
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                if d.description().is_ok_and(|desc| desc.name() == name) {
                    return Some(d);
                }
            }
        }
        eprintln!("[audio] micrófono «{name}» no encontrado; uso el predeterminado");
    }
    host.default_input_device()
}

pub struct Recorder {
    stop_tx: Sender<()>,
    join: Option<JoinHandle<Vec<f32>>>,
}

impl Recorder {
    /// Abre el micrófono y empieza a grabar. Devuelve cuando el stream ya está en marcha.
    pub fn start(mic: Option<String>, on_level: LevelFn) -> Result<Self, String> {
        let (stop_tx, stop_rx) = bounded::<()>(1);
        let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
        let join = std::thread::Builder::new()
            .name("pronto-audio".into())
            .spawn(move || run(mic, on_level, stop_rx, ready_tx))
            .map_err(|e| format!("no se pudo lanzar el hilo de audio: {e}"))?;
        match ready_rx.recv_timeout(Duration::from_secs(6)) {
            Ok(Ok(())) => Ok(Self { stop_tx, join: Some(join) }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                let _ = stop_tx.send(());
                Err("el micrófono no respondió".into())
            }
        }
    }

    /// Para y devuelve el audio mono a 16 kHz.
    pub fn stop(mut self) -> Vec<f32> {
        let _ = self.stop_tx.send(());
        self.join.take().and_then(|j| j.join().ok()).unwrap_or_default()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Si se abandona sin `stop`, el hilo termina solo al cerrarse el canal.
        let _ = self.stop_tx.try_send(());
    }
}

fn run(mic: Option<String>, on_level: LevelFn, stop_rx: crossbeam_channel::Receiver<()>, ready_tx: Sender<Result<(), String>>) -> Vec<f32> {
    let host = cpal::default_host();
    let Some(device) = pick_device(&host, mic.as_deref()) else {
        let _ = ready_tx.send(Err("No se encontró ningún micrófono".into()));
        return Vec::new();
    };
    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("No se pudo abrir el micrófono: {e}")));
            return Vec::new();
        }
    };
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let stream_config = config.config();
    let buf: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));

    let built = match format {
        SampleFormat::F32 => build::<f32>(&device, stream_config, channels, rate, buf.clone(), on_level),
        SampleFormat::I16 => build::<i16>(&device, stream_config, channels, rate, buf.clone(), on_level),
        SampleFormat::I32 => build::<i32>(&device, stream_config, channels, rate, buf.clone(), on_level),
        SampleFormat::U16 => build::<u16>(&device, stream_config, channels, rate, buf.clone(), on_level),
        SampleFormat::I8 => build::<i8>(&device, stream_config, channels, rate, buf.clone(), on_level),
        SampleFormat::U8 => build::<u8>(&device, stream_config, channels, rate, buf.clone(), on_level),
        other => Err(format!("formato de audio no soportado: {other}")),
    };
    let stream = match built {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(format!("No se pudo abrir el micrófono: {e}")));
            return Vec::new();
        }
    };
    if let Err(e) = stream.play() {
        let _ = ready_tx.send(Err(format!("No se pudo iniciar la captura: {e}")));
        return Vec::new();
    }
    let _ = ready_tx.send(Ok(()));

    // Bloquea hasta `stop` (o hasta que se descarte el Recorder).
    let _ = stop_rx.recv();
    drop(stream);

    let native = std::mem::take(&mut *buf.lock());
    resample_to_16k(&native, rate)
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    rate: u32,
    buf: Arc<Mutex<Vec<f32>>>,
    on_level: LevelFn,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let max_samples = rate as usize * MAX_SECONDS;
    let mut sum_sq = 0.0f64;
    let mut count = 0usize;
    let mut last_emit = Instant::now();
    let mut mono_chunk: Vec<f32> = Vec::new();

    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                mono_chunk.clear();
                for frame in data.chunks(channels.max(1)) {
                    let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                    mono_chunk.push(sum / frame.len() as f32);
                }
                for &s in &mono_chunk {
                    sum_sq += (s as f64) * (s as f64);
                }
                count += mono_chunk.len();
                {
                    let mut b = buf.lock();
                    if b.len() < max_samples {
                        b.extend_from_slice(&mono_chunk);
                    }
                }
                if last_emit.elapsed() >= LEVEL_EVERY && count > 0 {
                    let rms = (sum_sq / count as f64).sqrt() as f32;
                    on_level(rms_to_level(rms));
                    sum_sq = 0.0;
                    count = 0;
                    last_emit = Instant::now();
                }
            },
            |e| eprintln!("[audio] error de stream: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ captura continua (reuniones)

/// Qué fuente se abre en una reunión.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceKind {
    /// Micrófono elegido en Ajustes (o el predeterminado).
    Mic,
    /// Audio que suena por el dispositivo de salida predeterminado (WASAPI loopback).
    System,
}

/// Una fuente abierta: el `Stream` (que no es `Send`: hay que mantenerlo vivo en el hilo que lo
/// creó) y la frecuencia nativa de los trozos mono que llegan por el canal.
pub struct OpenSource {
    pub stream: cpal::Stream,
    pub rate: u32,
}

/// Abre `kind` y empieza a mandar trozos mono f32 (a la frecuencia nativa) por `tx`.
///
/// El loopback sale de abrir como *entrada* el dispositivo de *salida*: cpal activa
/// `AUDCLNT_STREAMFLAGS_LOOPBACK` solo. Ojo: WASAPI no entrega paquetes mientras no suena nada,
/// así que un canal en silencio puede pasar segundos sin recibir nada; quien lo consuma debe
/// tratarlo como silencio.
pub fn open_source(kind: SourceKind, mic: Option<&str>, tx: Sender<Vec<f32>>) -> Result<OpenSource, String> {
    let host = cpal::default_host();
    let (device, config) = match kind {
        SourceKind::Mic => {
            let d = pick_device(&host, mic).ok_or("No se encontró ningún micrófono")?;
            let c = d.default_input_config().map_err(|e| format!("No se pudo abrir el micrófono: {e}"))?;
            (d, c)
        }
        SourceKind::System => {
            let d = host.default_output_device().ok_or("No hay dispositivo de salida de audio")?;
            let c = d.default_output_config().map_err(|e| format!("No se pudo leer el formato de la salida: {e}"))?;
            (d, c)
        }
    };
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let cfg = config.config();
    let stream = match format {
        SampleFormat::F32 => chunk_stream::<f32>(&device, cfg, channels, tx),
        SampleFormat::I16 => chunk_stream::<i16>(&device, cfg, channels, tx),
        SampleFormat::I32 => chunk_stream::<i32>(&device, cfg, channels, tx),
        SampleFormat::U16 => chunk_stream::<u16>(&device, cfg, channels, tx),
        SampleFormat::I8 => chunk_stream::<i8>(&device, cfg, channels, tx),
        SampleFormat::U8 => chunk_stream::<u8>(&device, cfg, channels, tx),
        other => Err(format!("formato de audio no soportado: {other}")),
    }?;
    stream.play().map_err(|e| format!("No se pudo iniciar la captura: {e}"))?;
    Ok(OpenSource { stream, rate })
}

fn chunk_stream<T>(device: &cpal::Device, config: cpal::StreamConfig, channels: usize, tx: Sender<Vec<f32>>) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let chunk: Vec<f32> = data
                    .chunks(channels.max(1))
                    .map(|frame| frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / frame.len() as f32)
                    .collect();
                let _ = tx.send(chunk);
            },
            |e| eprintln!("[audio] error de stream: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// WAV PCM16 mono 16 kHz que se va escribiendo a disco según llega el audio (una reunión de una
/// hora no cabe cómoda en memoria). `finish` rellena las longitudes de la cabecera.
pub struct WavStream {
    file: std::io::BufWriter<std::fs::File>,
    bytes: u32,
}

impl WavStream {
    pub fn create(path: &std::path::Path) -> std::io::Result<Self> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        file.write_all(&encode_wav(&[])[..44])?;
        Ok(Self { file, bytes: 0 })
    }

    pub fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        use std::io::Write;
        for &s in samples {
            self.file.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
        self.bytes = self.bytes.saturating_add(samples.len() as u32 * 2);
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<()> {
        use std::io::{Seek, SeekFrom, Write};
        self.file.flush()?;
        let f = self.file.get_mut();
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&(36 + self.bytes).to_le_bytes())?;
        f.seek(SeekFrom::Start(40))?;
        f.write_all(&self.bytes.to_le_bytes())?;
        f.flush()
    }
}

/// RMS -> 0..1 en escala logarítmica con ganancia: -55 dBFS = 0, -10 dBFS = 1, de modo que una
/// voz normal (~-30 dBFS) llegue hacia la mitad y se vea bien en la Flow bar.
pub fn rms_to_level(rms: f32) -> f32 {
    let db = 20.0 * rms.max(1e-5).log10();
    ((db + 55.0) / 45.0).clamp(0.0, 1.0)
}

pub fn resample_to_16k(samples: &[f32], rate: u32) -> Vec<f32> {
    if samples.is_empty() || rate == TARGET_RATE {
        return samples.to_vec();
    }
    match sherpa_onnx::LinearResampler::create(rate as i32, TARGET_RATE as i32) {
        Some(r) => r.resample(samples, true),
        None => {
            // Plan B: interpolación lineal simple.
            let ratio = rate as f64 / TARGET_RATE as f64;
            let n = (samples.len() as f64 / ratio) as usize;
            (0..n)
                .map(|i| {
                    let pos = i as f64 * ratio;
                    let i0 = pos as usize;
                    let frac = (pos - i0 as f64) as f32;
                    let a = samples[i0];
                    let b = samples.get(i0 + 1).copied().unwrap_or(a);
                    a + (b - a) * frac
                })
                .collect()
        }
    }
}

/// WAV PCM 16 bit mono a 16 kHz (el formato del audio guardado por dictado).
pub fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&TARGET_RATE.to_le_bytes());
    out.extend_from_slice(&(TARGET_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Lee un WAV como el que escribe `encode_wav` (PCM16 mono). Devuelve `None` si no lo es.
pub fn decode_wav(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        if id == b"data" {
            let end = (body + len).min(bytes.len());
            return Some(bytes[body..end].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect());
        }
        pos = body + len + (len & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nivel_en_rango() {
        assert_eq!(rms_to_level(0.0), 0.0);
        assert_eq!(rms_to_level(1.0), 1.0);
        let voz = rms_to_level(0.03);
        assert!(voz > 0.3 && voz < 0.8, "voz = {voz}");
    }

    #[test]
    fn wav_ida_y_vuelta() {
        let src: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.02).sin() * 0.5).collect();
        let wav = encode_wav(&src);
        assert_eq!(wav.len(), 44 + 3200);
        let back = decode_wav(&wav).expect("wav válido");
        assert_eq!(back.len(), src.len());
        assert!(src.iter().zip(&back).all(|(a, b)| (a - b).abs() < 1e-3));
        assert!(decode_wav(b"no es un wav").is_none());
    }

    #[test]
    fn wav_en_streaming_es_un_wav_valido() {
        let path = std::env::temp_dir().join(format!("pronto_wavstream_{}.wav", std::process::id()));
        let src: Vec<f32> = (0..3200).map(|i| (i as f32 * 0.05).sin() * 0.4).collect();
        let mut w = WavStream::create(&path).unwrap();
        w.write(&src[..1000]).unwrap();
        w.write(&src[1000..]).unwrap();
        w.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(bytes, encode_wav(&src));
    }

    /// Abre el loopback del dispositivo de salida 1,5 s y cuenta lo recibido (sin guardar nada).
    /// `cargo test loopback -- --ignored --nocapture`. Con la salida en silencio puede llegar 0.
    #[test]
    #[ignore]
    fn loopback_se_abre() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let src = open_source(SourceKind::System, None, tx).expect("abrir loopback");
        std::thread::sleep(Duration::from_millis(1500));
        drop(src.stream);
        let total: usize = rx.try_iter().map(|c| c.len()).sum();
        eprintln!("loopback: {} Hz, {total} muestras en 1,5 s", src.rate);
    }

    #[test]
    fn remuestreo_conserva_duracion() {
        let one_sec: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin()).collect();
        let out = resample_to_16k(&one_sec, 48_000);
        assert!((out.len() as i64 - 16_000).abs() < 200, "len = {}", out.len());
        assert_eq!(resample_to_16k(&one_sec[..100], 16_000).len(), 100);
    }
}
