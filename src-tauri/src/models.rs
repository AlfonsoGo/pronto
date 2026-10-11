//! Tipos compartidos del contrato IPC (JSON en camelCase).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    Formal,
    Casual,
    VeryCasual,
    Excited,
}

/// Nivel de limpieza automática (local y determinista, sin LLM).
/// Qué hace Pronto cuando detecta una reunión: tomar notas solo (`auto`), preguntar (`ask`) o nada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MeetingAuto {
    Auto,
    Ask,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupLevel {
    None,
    Light,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Styles {
    pub personal: Tone,
    pub work: Tone,
    pub email: Tone,
    pub other: Tone,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            personal: Tone::Casual,
            work: Tone::Casual,
            email: Tone::Formal,
            other: Tone::Casual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: Option<String>,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self { ctrl: false, alt: false, shift: false, win: false, key: None }
    }
}

impl Hotkey {
    /// Texto legible para avisos: «Win + Alt + Espacio».
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.win {
            parts.push("Win".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Mayús".into());
        }
        if let Some(k) = self.key.as_deref() {
            parts.push(if k.eq_ignore_ascii_case("space") { "Espacio".into() } else { k.to_string() });
        }
        parts.join(" + ")
    }
}

/// Atajo de fábrica del Transform 1 (Alt+Shift+P, junto a Alt+Shift+Z/X).
pub fn default_transform_hotkey() -> Hotkey {
    Hotkey { alt: true, shift: true, key: Some("P".into()), ..Hotkey::default() }
}

/// Atajo de fábrica anterior (Win+Alt+1): chocaba con el dictado «mantener» (Win+Alt).
pub fn legacy_transform_hotkey() -> Hotkey {
    Hotkey { win: true, alt: true, key: Some("1".into()), ..Hotkey::default() }
}

/// Nombre de usuario de Windows con la primera en mayúscula (valor por defecto de `firstName`).
fn windows_first_name() -> String {
    let raw = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkey_hold: Hotkey,
    pub hotkey_hands_free: Hotkey,
    pub hotkey_paste_last: Hotkey,
    pub hotkey_copy_last: Hotkey,
    /// Transform 1 (Polish). Si la tecla es un dígito, los siguientes dígitos lanzan Transform 2 y 3.
    pub hotkey_transform: Hotkey,
    /// Empezar / parar notas de reunión (Win+Alt+M).
    pub hotkey_meeting: Hotkey,
    /// Obsoleto: equivale a `meeting_auto != Off`. Se mantiene sincronizado por compatibilidad.
    pub meeting_detect: bool,
    /// Qué hacer al detectar una reunión (por defecto, tomar notas solo).
    pub meeting_auto: MeetingAuto,
    /// Guardar el audio (mic y sistema) de las reuniones.
    pub meeting_keep_audio: bool,
    pub first_name: String,
    pub mic_name: Option<String>,
    pub language: String,
    pub app_language: String,
    pub model_dir: String,
    pub launch_at_login: bool,
    pub show_flow_bar: bool,
    pub sounds: bool,
    pub mute_music: bool,
    /// Obsoletos: solo se leen para migrar a `cleanup_level`. Se mantienen por compatibilidad.
    pub remove_fillers: bool,
    pub backtrack: bool,
    pub cleanup_level: CleanupLevel,
    pub press_enter_command: bool,
    pub ollama_url: String,
    pub ollama_model: String,
    /// «Pulir con IA siempre»: cada dictado pasa por Ollama antes de pegarse.
    pub ai_polish: bool,
    /// Modelo del pulido (vacío = el de `ollama_model`).
    pub ai_polish_model: String,
    /// Plazo duro del pulido; si vence se pega sin pulir.
    pub ai_polish_timeout_ms: u32,
    pub keep_audio_days: u32,
    pub theme: String,
    pub flow_bar_hidden_until: Option<String>,
    pub styles: Styles,
    pub history_retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let model_dir = crate::modeldl::default_dir();
        Self {
            hotkey_hold: Hotkey { win: true, alt: true, ..Hotkey::default() },
            hotkey_hands_free: Hotkey { win: true, alt: true, key: Some("Space".into()), ..Hotkey::default() },
            hotkey_paste_last: Hotkey { alt: true, shift: true, key: Some("Z".into()), ..Hotkey::default() },
            hotkey_copy_last: Hotkey { alt: true, shift: true, key: Some("X".into()), ..Hotkey::default() },
            hotkey_transform: default_transform_hotkey(),
            hotkey_meeting: Hotkey { win: true, alt: true, key: Some("M".into()), ..Hotkey::default() },
            meeting_detect: true,
            meeting_auto: MeetingAuto::Auto,
            meeting_keep_audio: false,
            first_name: windows_first_name(),
            mic_name: None,
            language: "auto".into(),
            app_language: "es".into(),
            model_dir,
            launch_at_login: false,
            show_flow_bar: true,
            sounds: true,
            mute_music: true,
            remove_fillers: true,
            backtrack: true,
            cleanup_level: CleanupLevel::Medium,
            press_enter_command: false,
            ollama_url: "http://localhost:11434".into(),
            ollama_model: "qwen2.5:7b".into(),
            ai_polish: false,
            ai_polish_model: "qwen2.5:7b".into(),
            ai_polish_timeout_ms: 6000,
            keep_audio_days: 14,
            theme: "dark".into(),
            flow_bar_hidden_until: None,
            styles: Styles::default(),
            history_retention_days: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub created_at: String,
    pub text: String,
    pub raw_text: String,
    pub app: String,
    pub duration_ms: i64,
    pub words: i64,
    pub has_audio: bool,
    /// El texto pasó por «Pulir con IA»; `raw_text` guarda entonces el texto antes de pulir.
    pub polished: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayWords {
    pub date: String,
    pub words: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total_words: i64,
    pub wpm: f64,
    pub streak_days: i64,
    pub dictations: i64,
    pub words_this_week: i64,
    pub by_day: Vec<DayWords>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub id: i64,
    pub word: String,
    pub replacement: Option<String>,
    pub created_at: String,
    pub starred: bool,
    /// `manual` | `auto`
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub hotkey_index: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub app: String,
    pub words: i64,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fixes {
    pub dictionary: i64,
    pub snippets: i64,
    pub fillers: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insights {
    pub total_words: i64,
    pub wpm: f64,
    pub streak_days: i64,
    pub longest_streak: i64,
    pub dictations: i64,
    pub heatmap: Vec<DayWords>,
    pub top_apps: Vec<AppUsage>,
    pub fixes: Fixes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaStatus {
    pub reachable: bool,
    pub models: Vec<String>,
}

/// Rectángulo pulsable de la Flow bar, en px CSS relativos a la ventana.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub text: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub ready: bool,
    pub loading: bool,
    pub model_dir: String,
    pub error: Option<String>,
}

/// Estado de un fichero del modelo de voz en disco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileStatus {
    pub name: String,
    /// Está entero (tamaño esperado) y verificado al descargarse.
    pub ok: bool,
    /// Bytes en disco: el fichero final o, si se está bajando, su `.part`.
    pub bytes: u64,
    /// Tamaño esperado.
    pub size: u64,
}

/// Respuesta de `model_status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub dir: String,
    pub complete: bool,
    pub downloading: bool,
    /// `idle` | `downloading` | `verifying` | `done` | `error`
    pub state: &'static str,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub overall_percent: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub files: Vec<ModelFileStatus>,
}

/// Payload de `model://progress`. `done_bytes`/`total_bytes` cuentan los cuatro ficheros juntos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub file: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub overall_percent: u32,
    /// `downloading` | `verifying` | `done` | `error`
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Payload de `dictation://state`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationState {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// `tap` | `nomic` | `noaudio` | `paste` | `ollama` | `select`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
}

/// Payload de `dictation://notice` (tarjetas sobre la Flow bar).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

// ---------------------------------------------------------------- Notetaker (reuniones)

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: i64,
    pub app: Option<String>,
    /// `none` | `pending` | `done` | `failed`
    pub summary_status: String,
    /// `true` si la arrancó la detección automática; `false` si fue a mano (botón, atajo, aviso).
    pub auto: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingListItem {
    #[serde(flatten)]
    pub meeting: Meeting,
    pub preview: String,
    pub action_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub meeting_id: i64,
    /// `me` (micrófono) | `them` (audio del sistema)
    pub speaker: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub text: String,
    pub owner: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetail {
    #[serde(flatten)]
    pub meeting: Meeting,
    pub segments: Vec<Segment>,
    pub summary: Option<String>,
    pub action_items: Vec<ActionItem>,
    pub decisions: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingStatus {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting: Option<Meeting>,
    pub elapsed_ms: i64,
}
