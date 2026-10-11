//! Cliente mínimo de Ollama (local, sin TLS): `POST /api/generate` y `GET /api/tags`.
//! Usa `ureq` en bloqueante; llamar siempre fuera del hilo del hook y del hilo principal.

use crate::models::OllamaStatus;
use serde_json::{json, Value};
use std::time::Duration;

fn base(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn agent(read_timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).timeout(read_timeout).build()
}

/// Ejecuta `prompt` sobre `text` con el modelo indicado y devuelve la respuesta ya recortada.
pub fn generate(url: &str, model: &str, prompt: &str, text: &str) -> Result<String, String> {
    generate_raw(url, model, &format!("{prompt}\n\nTexto:\n{text}"), false, None)
}

/// Envía `prompt` tal cual. Con `json` pide a Ollama `format: "json"` (salida JSON válida) y con
/// `num_ctx` amplía la ventana de contexto (por defecto Ollama usa solo 2-4k tokens y recorta).
pub fn generate_raw(url: &str, model: &str, prompt: &str, json: bool, num_ctx: Option<u32>) -> Result<String, String> {
    let opts = GenOpts { json, num_ctx, timeout: Duration::from_secs(600), keep_alive: None, temperature: None };
    generate_with(url, model, prompt, &opts)
}

/// Opciones de una generación.
pub struct GenOpts<'a> {
    pub json: bool,
    pub num_ctx: Option<u32>,
    /// Plazo duro de toda la petición.
    pub timeout: Duration,
    /// Cuánto mantiene Ollama el modelo cargado tras responder (p. ej. `"30m"`).
    pub keep_alive: Option<&'a str>,
    pub temperature: Option<f32>,
}

pub fn generate_with(url: &str, model: &str, prompt: &str, opts: &GenOpts) -> Result<String, String> {
    let mut body = json!({ "model": model, "prompt": prompt, "stream": false, "think": false });
    if opts.json {
        body["format"] = json!("json");
    }
    let mut options = serde_json::Map::new();
    if let Some(n) = opts.num_ctx {
        options.insert("num_ctx".into(), json!(n));
    }
    if let Some(t) = opts.temperature {
        options.insert("temperature".into(), json!(t));
    }
    if !options.is_empty() {
        body["options"] = Value::Object(options);
    }
    if let Some(k) = opts.keep_alive {
        body["keep_alive"] = json!(k);
    }
    let endpoint = format!("{}/api/generate", base(url));
    let resp = agent(opts.timeout).post(&endpoint).send_json(body).map_err(|e| match e {
        ureq::Error::Status(code, r) => {
            let detail = r.into_json::<Value>().ok().and_then(|v| v["error"].as_str().map(String::from));
            match (code, detail) {
                (_, Some(d)) => format!("Ollama respondió: {d}"),
                (c, None) => format!("Ollama respondió con el código {c}"),
            }
        }
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Io if t.to_string().to_lowercase().contains("timed out") => "Ollama no respondió a tiempo".to_string(),
            _ => format!("No se pudo contactar con Ollama en {}", base(url)),
        },
    })?;
    let v: Value = resp.into_json().map_err(|e| format!("Respuesta de Ollama ilegible: {e}"))?;
    let out = v["response"].as_str().unwrap_or("").trim().to_string();
    if out.is_empty() {
        Err("Ollama devolvió una respuesta vacía".into())
    } else {
        Ok(out)
    }
}

/// ¿Responde Ollama y qué modelos tiene?
pub fn status(url: &str) -> OllamaStatus {
    let endpoint = format!("{}/api/tags", base(url));
    match agent(Duration::from_secs(3)).get(&endpoint).call().ok().and_then(|r| r.into_json::<Value>().ok()) {
        Some(v) => OllamaStatus {
            reachable: true,
            models: v["models"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
                .unwrap_or_default(),
        },
        None => OllamaStatus { reachable: false, models: Vec::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Servidor HTTP de un solo uso que responde `body` con `status` y devuelve lo que recibió.
    fn one_shot(status: &str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let status = status.to_string();
        let h = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = vec![0u8; 16384];
            let mut got = Vec::new();
            // Leemos hasta tener cabeceras y cuerpo completos (Content-Length).
            loop {
                let n = sock.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got).to_string();
                if let Some(i) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                        .unwrap_or(0);
                    if got.len() >= i + 4 + len || n == 0 {
                        break;
                    }
                }
            }
            let resp = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            sock.write_all(resp.as_bytes()).unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        (url, h)
    }

    #[test]
    fn genera_con_el_prompt_y_el_texto() {
        let (url, h) = one_shot("200 OK", r#"{"response":"  Hola mundo.\n"}"#);
        let out = generate(&url, "qwen2.5:7b", "Mejora esto", "hola mundo").unwrap();
        assert_eq!(out, "Hola mundo.");
        let req = h.join().unwrap();
        assert!(req.starts_with("POST /api/generate"), "{req}");
        assert!(req.contains(r#""stream":false"#) && req.contains("qwen2.5:7b"));
        assert!(req.contains("Mejora esto") && req.contains("hola mundo"));
    }

    #[test]
    fn pide_json_y_contexto() {
        let (url, h) = one_shot("200 OK", r#"{"response":"{\"a\":1}"}"#);
        let out = generate_raw(&url, "m", "dame json", true, Some(8192)).unwrap();
        assert_eq!(out, "{\"a\":1}");
        let req = h.join().unwrap();
        assert!(req.contains(r#""format":"json""#) && req.contains(r#""num_ctx":8192"#), "{req}");
    }

    #[test]
    fn error_de_ollama_se_explica() {
        let (url, h) = one_shot("404 Not Found", r#"{"error":"model 'x' not found"}"#);
        let err = generate(&url, "x", "p", "t").unwrap_err();
        assert!(err.contains("not found"), "{err}");
        h.join().unwrap();
    }

    #[test]
    fn lista_modelos() {
        let (url, h) = one_shot("200 OK", r#"{"models":[{"name":"qwen2.5:7b"},{"name":"llama3.2"}]}"#);
        let s = status(&url);
        assert!(s.reachable);
        assert_eq!(s.models, ["qwen2.5:7b", "llama3.2"]);
        h.join().unwrap();
    }

    #[test]
    fn sin_servidor_no_es_alcanzable() {
        // Puerto cerrado: se reserva y se libera.
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        assert!(!status(&url).reachable);
        assert!(generate(&url, "m", "p", "t").unwrap_err().contains("No se pudo contactar"));
    }
}
