//! Sondas del rompedor (2026-10-22, tanda paralela): reuniones. Solo se compilan en pruebas.
//! Las que demuestran una rotura llevan #[ignore = "rompedor: <título>"].
#![allow(dead_code, unused_imports)]

use crate::asr::{Timed, TimedToken};
use crate::audio::{self, WavReader, WavStream, TARGET_RATE};
use crate::auto::{AutoAction, AutoMachine, Recording};
use crate::db::Db;
use crate::models::*;
use crate::notes::{self, turns::Word, AsrOut};
use crate::summary::{self, ParsedAction, ParsedSummary};
use std::path::{Path, PathBuf};
use std::time::Instant;

// ------------------------------------------------------------------ utilidades

/// Carpeta temporal propia que se borra al terminar.
struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("rompedor-reuniones-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn join(&self, f: &str) -> PathBuf {
        self.0.join(f)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tone(ms: usize, amp: f32) -> Vec<f32> {
    (0..ms * 16).map(|i| (i as f32 * 0.13).sin() * amp).collect()
}

/// Reconocedor falso: una palabra por segundo de audio.
fn fake_asr(s: &[f32]) -> Result<AsrOut, String> {
    let secs = (s.len() / 16_000).max(1);
    let tokens: Vec<TimedToken> = (0..secs).map(|i| TimedToken { text: format!(" palabra{i}"), start: i as f32, end: i as f32 + 0.5 }).collect();
    Ok((Timed { text: tokens.iter().map(|t| t.text.as_str()).collect::<String>().trim().to_string(), tokens }, Vec::new()))
}

/// Habla sintética: tonos de 2 s separados por 2 s de silencio.
fn speech(secs: usize) -> Vec<f32> {
    let mut v = vec![0.0; 16_000];
    for _ in 0..secs / 4 {
        v.extend(tone(2_000, 0.2));
        v.extend(vec![0.0; 32_000]);
    }
    v
}

fn transcribe(path: &Path, who: &str) -> Result<(Vec<Word>, u64, usize), String> {
    notes::transcribe_words(path, who, &mut fake_asr, &mut |_| {})
}

fn seg(id: i64, who: &str, a: i64, b: i64, t: &str) -> Segment {
    Segment { meeting_id: id, speaker: who.into(), start_ms: a, end_ms: b, text: t.into(), ..Default::default() }
}

fn header(rate: u32, data_len: u32) -> Vec<u8> {
    let mut h = audio::encode_wav(&[]);
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

// ------------------------------------------------------------------ WAV raros

/// Reuniones de 0 s, de una muestra, WAV cortados por un cierre brusco y cabeceras que mienten:
/// ni pánico ni cuelgue, y lo que hay se lee.
#[test]
fn rompedor_reuniones_wav_raros_de_reunion() {
    let t = Tmp::new("wav");
    let mut out = Vec::new();
    // Reunión de 0 s: cabecera sola, cerrada bien.
    let p = t.join("cero.wav");
    WavStream::create_new(&p).unwrap().finish().unwrap();
    out.push(("cero_s", std::fs::metadata(&p).unwrap().len(), transcribe(&p, "them").map(|(w, n, s)| (w.len(), n, s))));
    // Una sola muestra.
    let p = t.join("una.wav");
    let mut w = WavStream::create_new(&p).unwrap();
    w.write(&[0.5]).unwrap();
    w.finish().unwrap();
    out.push(("una_muestra", std::fs::metadata(&p).unwrap().len(), transcribe(&p, "me").map(|(w, n, s)| (w.len(), n, s))));
    // Byte impar al final (corte a mitad de muestra).
    let p = t.join("impar.wav");
    let mut b = audio::encode_wav(&speech(8));
    b.push(0x7f);
    std::fs::write(&p, &b).unwrap();
    out.push(("byte_impar", b.len() as u64, transcribe(&p, "them").map(|(w, n, s)| (w.len(), n, s))));
    // Cabecera que dice 4 GB de datos.
    let p = t.join("miente_grande.wav");
    let mut b = header(TARGET_RATE, u32::MAX);
    b.extend(audio::encode_wav(&speech(8))[44..].iter());
    std::fs::write(&p, &b).unwrap();
    out.push(("cabecera_4gb", b.len() as u64, transcribe(&p, "them").map(|(w, n, s)| (w.len(), n, s))));
    // Cabecera que dice 0 B (cierre antes del primer volcado de 5 s).
    let p = t.join("miente_cero.wav");
    let mut b = header(TARGET_RATE, 0);
    b.extend(audio::encode_wav(&speech(8))[44..].iter());
    std::fs::write(&p, &b).unwrap();
    out.push(("cabecera_0", b.len() as u64, transcribe(&p, "them").map(|(w, n, s)| (w.len(), n, s))));
    // Cierre brusco a los 3 s (sin `finish` ni `Drop`: lo que quedó en el búfer se pierde).
    let p = t.join("brusco.wav");
    let mut w = WavStream::create_new(&p).unwrap();
    w.write(&speech(8)).unwrap();
    std::mem::forget(w);
    out.push(("cierre_brusco_8s", std::fs::metadata(&p).unwrap().len(), transcribe(&p, "them").map(|(w, n, s)| (w.len(), n, s))));
    // Frecuencia que no es 16 kHz y basura.
    let p = t.join("48k.wav");
    let mut b = header(48_000, 0);
    b.extend(vec![0u8; 9600]);
    std::fs::write(&p, &b).unwrap();
    out.push(("48k", b.len() as u64, transcribe(&p, "them").map(|(w, n, s)| (w.len(), n, s))));
    for (name, len, r) in &out {
        eprintln!("{name}: {len} B -> {r:?}");
    }
    assert!(out.iter().take(6).all(|(_, _, r)| r.is_ok()), "{out:?}");
}

// ------------------------------------------------------------------ transcripción parcial (arreglo de hoy)

/// Una pista que quedó en 0 bytes (la app o el PC se cayeron, o el hilo de esa pista se colgó al
/// abrir el dispositivo, antes del primer volcado del búfer de 8 KB del WAV). La otra pista está
/// sana. Se repite la fase 2 tal como la hace `meeting.rs:510-569` + `:584-602` tres veces
/// («Reintentar»): la transcripción nunca queda final y las notas nunca se generan.
#[test]
fn rompedor_reuniones_pista_vacia_bloquea_las_notas_para_siempre() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Pista rota", Some("Zoom"), true, true).unwrap();
    db.finish_meeting(m.id, 120_000).unwrap();
    let me = crate::meeting::audio_file(&m, "me").unwrap();
    let them = crate::meeting::audio_file(&m, "them").unwrap();
    // «Tú»: 2 min de habla, cerrado bien.
    let mut w = WavStream::create_new(&me).unwrap();
    w.write(&speech(120)).unwrap();
    w.finish().unwrap();
    // «Ellos»: el hilo crea el WAV y escribe algo, pero el proceso muere antes del primer volcado.
    let mut w = WavStream::create_new(&them).unwrap();
    w.write(&vec![0.1; 2_000]).unwrap();
    std::mem::forget(w);
    let them_len = std::fs::metadata(&them).unwrap().len();

    let mut log = Vec::new();
    for intento in 1..=3 {
        let files = [("them", &them), ("me", &me)];
        let (mut wme, mut wthem, mut failed, mut skipped) = (Vec::new(), Vec::new(), Vec::new(), 0);
        for (who, p) in files {
            match transcribe(p, who) {
                Ok((words, _, s)) => {
                    skipped += s;
                    if who == "me" { wme = words } else { wthem = words }
                }
                Err(e) => failed.push(e),
            }
        }
        // meeting.rs:501 `transcript_complete`
        let complete = !files.is_empty() && failed.is_empty() && skipped == 0;
        let (segs, _, _) = notes::conversation(wme, wthem, m.id, &|s: &str| Some(s.to_string()));
        db.replace_segments(m.id, &segs, complete).unwrap();
        log.push(format!("intento {intento}: turnos={} fallos={failed:?} final={}", segs.len(), db.transcript_final(m.id).unwrap()));
    }
    for l in &log {
        eprintln!("{l}");
    }
    eprintln!("WAV de «Ellos»: {them_len} B; transcribe_words -> {:?}", transcribe(&them, "them").map(|r| r.1));
    crate::meeting::remove_audio_files(&m);
    assert!(
        transcribe(&them, "them").is_ok() || db.transcript_final(m.id).unwrap(),
        "una pista sin audio (0 B) debería contar como pista vacía: si no, «Reintentar» nunca saca las notas ({log:?})"
    );
}

// ------------------------------------------------------------------ dos reuniones seguidas

/// Dos reuniones seguidas sin pausa: cada una con sus WAV; borrar la primera no toca los de la segunda.
#[test]
fn rompedor_reuniones_dos_seguidas_no_comparten_audio() {
    let db = Db::open_memory().unwrap();
    let a = db.create_meeting_titled("A", None, false, false).unwrap();
    db.finish_meeting(a.id, 1_000).unwrap();
    let b = db.create_meeting_titled("B", None, false, false).unwrap();
    let paths: Vec<PathBuf> = ["me", "them"].iter().flat_map(|w| [crate::meeting::audio_file(&a, w).unwrap(), crate::meeting::audio_file(&b, w).unwrap()]).collect();
    for p in &paths {
        let mut w = WavStream::create_new(p).unwrap();
        w.write(&tone(500, 0.2)).unwrap();
        w.finish().unwrap();
    }
    crate::meeting::remove_audio_files(&a);
    let quedan: Vec<bool> = paths.iter().map(|p| p.is_file()).collect();
    crate::meeting::remove_audio_files(&b);
    eprintln!("{paths:?} -> {quedan:?}");
    assert_eq!(quedan, [false, true, false, true]);
}

// ------------------------------------------------------------------ detección automática

/// PRD §1 «decidido»: no graba reuniones de Discord ni de juegos; solo Teams, Zoom y Meet (+R1).
/// La detección compara por subcadena del exe y por palabra del título de cualquier ventana del
/// navegador.
#[test]
fn rompedor_reuniones_falsos_positivos_de_la_deteccion() {
    let mic = |exe: &str| vec![(exe.to_string(), true)];
    let casos: Vec<(&str, Vec<String>)> = vec![
        ("TeamSpeak (chat de voz de juegos)", crate::detect::detect_all(&mic("C:#Program Files#TeamSpeak#TeamSpeak.exe"), &[])),
        ("ZoomIt (grabar pantalla con micro)", crate::detect::detect_all(&mic("C:#Tools#ZoomIt64.exe"), &[])),
        (
            "Discord en Chrome, canal #teams",
            crate::detect::detect_all(&mic("C:#Program Files#Google#Chrome#Application#chrome.exe"), &[("chrome.exe".into(), "#teams | Mi servidor - Discord - Google Chrome".into())]),
        ),
        (
            "Llamada de WhatsApp Web con otra ventana de Chrome leyendo un artículo",
            crate::detect::detect_all(
                &mic("C:#x#chrome.exe"),
                &[("chrome.exe".into(), "WhatsApp - Google Chrome".into()), ("chrome.exe".into(), "Cómo hacer zoom en fotos - Google Chrome".into())],
            ),
        ),
    ];
    for (caso, got) in &casos {
        eprintln!("{caso}: {got:?}");
    }
    assert!(casos.iter().all(|(_, got)| got.is_empty()), "{casos:?}");
}

/// R3: «las iniciadas a mano nunca se paran solas». El usuario para a mano la reunión automática
/// de Teams y, antes del siguiente vistazo del detector (3 s), empieza otra a mano (p. ej. para
/// ponerle título). La máquina no ve el hueco y para la manual 20 s después de colgar.
#[test]
fn rompedor_reuniones_manual_tras_parar_la_auto_se_para_sola() {
    let teams = vec!["Microsoft Teams".to_string()];
    let mut m = AutoMachine::new();
    let a = m.step(3_000, MeetingAuto::Auto, Recording::Idle, &teams);
    assert_eq!(a, Some(AutoAction::Start("Microsoft Teams".into())));
    // t=3,5 s parada a mano y t=4 s nueva a mano: en t=6 s el detector ve «grabando» como siempre.
    // (Ajuste de API, no de premisa: `step` recibe ahora qué se graba, `Recording`, en vez de un bool;
    // lo que se graba desde t=4 s es la reunión manual.)
    let mut stop_at = None;
    for t in (6_000..120_000).step_by(3_000) {
        let in_use = if t < 60_000 { teams.clone() } else { Vec::new() };
        if let Some(AutoAction::Stop) = m.step(t, MeetingAuto::Auto, Recording::Manual, &in_use) {
            stop_at = Some(t);
            break;
        }
    }
    eprintln!("la reunión manual se para sola en t={stop_at:?} ms");
    assert_eq!(stop_at, None);
}

// ------------------------------------------------------------------ reunión de 3 h

/// Palabras seguidas desde `t0`, 300 ms cada una con 50 ms de hueco.
fn ws(t0: i64, words: &[&str]) -> Vec<Word> {
    words.iter().enumerate().map(|(i, p)| Word { start_ms: t0 + i as i64 * 350, end_ms: t0 + i as i64 * 350 + 300, text: p.to_string() }).collect()
}

const ELLOS: &[&str] = &[
    "presupuesto", "trimestre", "proveedor", "contrato", "entrega", "factura", "cliente", "campaña", "objetivo", "margen", "almacén", "pedido", "precio",
    "calendario", "informe", "reunión", "equipo", "proyecto", "producto", "ventas",
];
const YO: &[&str] = &["espera", "perdona", "pregunta", "concreta", "sobre", "eso"];

/// Reunión de 3 h en la que «Ellos» habla casi sin parar (turnos de 7 s con 1 s de pausa) y «Tú»
/// mete una frase con sustancia a mitad de cada turno: cada una debe partir el turno de «Ellos».
fn tres_horas(turn_ms: i64, total_ms: i64) -> (Vec<Word>, Vec<Word>, Vec<i64>) {
    let (mut me, mut them, mut cuts) = (Vec::new(), Vec::new(), Vec::new());
    let mut t = 0;
    let mut k = 0;
    while t + turn_ms < total_ms {
        let n = ((turn_ms - 1_000) / 350) as usize;
        let words: Vec<&str> = (0..n).map(|i| ELLOS[(k + i) % ELLOS.len()]).collect();
        them.extend(ws(t, &words));
        let at = t + (turn_ms - 1_000) / 2;
        me.extend(ws(at, &YO[k % 3..k % 3 + 3]));
        cuts.push(at);
        t += turn_ms;
        k += 1;
    }
    (me, them, cuts)
}

#[test]
fn rompedor_reuniones_tres_horas_de_interrupciones() {
    let (me, them, cuts) = tres_horas(8_000, 3 * 3_600_000);
    let (n_me, n_them) = (me.len(), them.len());
    let t0 = Instant::now();
    let (segs, _, stats) = notes::conversation(me, them, 1, &|s: &str| Some(s.to_string()));
    let t_conv = t0.elapsed();
    // Una interrupción quedó bien colocada si ningún turno de «Ellos» la cruza.
    let unsplit: Vec<i64> = cuts.iter().copied().filter(|&at| segs.iter().any(|s| s.speaker == "them" && s.start_ms < at && s.end_ms > at + 1_000)).collect();
    eprintln!(
        "palabras: tú {n_me}, ellos {n_them}; interrupciones {}; turnos {}; eco {:?}; conversation() {:.1} s (perfil test); sin partir: {} (la primera en {} s)",
        cuts.len(),
        segs.len(),
        stats,
        t_conv.as_secs_f32(),
        unsplit.len(),
        unsplit.first().map_or(-1, |ms| ms / 1000)
    );
    // Lo que viene después (etiquetas y comprensión) con un Ollama falso que contesta al instante.
    let mut segs = segs;
    let mut calls = 0;
    let mut llm = |_: &str, _: f32| -> Result<String, String> {
        calls += 1;
        Ok(r#"{"titulo":"Presupuesto","resumen":"Se habló del presupuesto del trimestre.","tareas":[],"decisiones":[],"temas":["presupuesto"],"hechos":["presupuesto del trimestre"]}"#.into())
    };
    let t1 = Instant::now();
    let _ = notes::label_turns(&mut llm, &mut segs, "Txonso", &mut |_, _| {});
    let t_label = t1.elapsed();
    let t2 = Instant::now();
    let r = notes::understand(&mut llm, &segs, "Txonso", &mut |_, _| {});
    eprintln!(
        "label_turns {:.1} s, understand {:.1} s, {calls} llamadas al modelo, notas {}",
        t_label.as_secs_f32(),
        t2.elapsed().as_secs_f32(),
        r.as_ref().map(|n| n.summary.len()).unwrap_or(0)
    );
    assert!(unsplit.is_empty(), "{} interrupciones sin colocar", unsplit.len());
}

// ------------------------------------------------------------------ exportar

/// Lo exportado es Markdown sin escapar: un título o un texto con saltos de línea, enlaces o HTML
/// crean secciones y tareas falsas y enlaces activos al pegarlo en Notion/Obsidian.
#[test]
fn rompedor_reuniones_exportar_con_texto_hostil() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("", Some("Zoom"), true, true).unwrap();
    db.replace_segments(m.id, &[seg(m.id, "them", 0, 3_000, "Mira [la factura](javascript:alert(1)) <img src=x onerror=alert(1)> ya está")], true).unwrap();
    let s = ParsedSummary {
        title: Some("Revisión\n\n## Tareas\n\n- [x] Transferir 5.000 € a ES00 0000".into()),
        summary: "Se revisó la factura.".into(),
        key_points: vec!["Ver [detalle](https://ejemplo.invalid/pago)\n## Decisiones\n- Pagar hoy".into()],
        ..Default::default()
    };
    db.save_summary(m.id, &s).unwrap();
    let md = summary::to_markdown(&db.get_meeting_detail(m.id).unwrap().unwrap());
    eprintln!("{md}");
    let titulo = db.get_meeting(m.id).unwrap().unwrap().title;
    assert!(
        !md.contains("](javascript:") && md.matches("\n## Tareas").count() == 0 && !titulo.contains('\n'),
        "secciones o enlaces inyectados"
    );
}

// ------------------------------------------------------------------ regenerar

/// «Regenerar» las notas borra las marcas de las tareas que el usuario ya había hecho, aunque la
/// tarea vuelva a salir igual.
#[test]
fn rompedor_reuniones_regenerar_desmarca_tareas() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Daily", None, false, false).unwrap();
    db.finish_meeting(m.id, 60_000).unwrap();
    let s = ParsedSummary {
        summary: "Daily.".into(),
        action_items: vec![ParsedAction { text: "Enviar el contrato a Ana".into(), owner: Some("Txonso".into()), due: Some("el lunes".into()) }],
        ..Default::default()
    };
    db.save_summary(m.id, &s).unwrap();
    db.toggle_action_item(m.id, 0).unwrap();
    let antes = db.get_meeting_detail(m.id).unwrap().unwrap().action_items[0].done;
    db.save_summary(m.id, &s).unwrap(); // «Regenerar»
    let despues = db.get_meeting_detail(m.id).unwrap().unwrap().action_items[0].done;
    eprintln!("hecha antes de regenerar: {antes}; después: {despues}");
    assert!(despues);
}

// ------------------------------------------------------------------ búsqueda

/// FTS con sintaxis rara, títulos enormes y unicode raro: sin errores y lo guardado se relee igual.
#[test]
fn rompedor_reuniones_busqueda_y_titulos_hostiles() {
    let db = Db::open_memory().unwrap();
    let largo = "Presupuesto ".repeat(850) + "\u{202E}gnp.exe\u{202C} \u{1F468}\u{200D}\u{1F469} e\u{301} \u{200B}\0fin";
    let m = db.create_meeting_titled(&largo, None, false, false).unwrap();
    db.replace_segments(m.id, &[seg(m.id, "me", 0, 1_000, "hablamos del presupuesto y del margen")], true).unwrap();
    assert_eq!(db.get_meeting(m.id).unwrap().unwrap().title, largo.trim());
    let qs = [
        "\"", "*", "NEAR(a b)", "-presupuesto", "presupuesto -margen", "content:presupuesto", "meeting_id:1", "kind:seg", "a AND NOT b", "(presupuesto", "presupuesto)",
        "{presupuesto}", "presu*", "^presu", "%", "_", "\\", "' OR 1=1 --", "\u{1F468}\u{200D}\u{1F469}", "é\u{301}", "\u{200B}", "\0",
    ];
    let mut errs = Vec::new();
    for q in qs {
        match db.list_meetings(Some(q), 50, 0) {
            Ok(r) => eprintln!("{q:?} -> {}", r.len()),
            Err(e) => errs.push(format!("list {q:?}: {e}")),
        }
        if let Err(e) = db.rank_meetings(q, 4) {
            errs.push(format!("rank {q:?}: {e}"));
        }
    }
    db.rename_meeting(m.id, &"x".repeat(1_000_000)).unwrap();
    let md = summary::to_markdown(&db.get_meeting_detail(m.id).unwrap().unwrap());
    assert!(md.len() > 1_000_000);
    assert!(errs.is_empty(), "{errs:?}");
}

/// Buscar «IA», «Q3» o «KPI»: las palabras de menos de 3 letras no se buscan en la transcripción, y
/// «plan IA» devuelve reuniones que no hablan de IA.
#[test]
fn rompedor_reuniones_busqueda_ignora_palabras_cortas() {
    let db = Db::open_memory().unwrap();
    let a = db.create_meeting_titled("Lunes", None, false, false).unwrap();
    db.replace_segments(a.id, &[seg(a.id, "them", 0, 2_000, "el plan de IA para el Q3")], true).unwrap();
    let b = db.create_meeting_titled("Martes", None, false, false).unwrap();
    db.replace_segments(b.id, &[seg(b.id, "them", 0, 2_000, "el plan de vacaciones")], true).unwrap();
    let ia = db.list_meetings(Some("IA"), 50, 0).unwrap().len();
    let q3 = db.list_meetings(Some("Q3"), 50, 0).unwrap().len();
    let plan_ia: Vec<String> = db.list_meetings(Some("plan IA"), 50, 0).unwrap().into_iter().map(|i| i.meeting.title).collect();
    eprintln!("«IA» -> {ia}; «Q3» -> {q3}; «plan IA» -> {plan_ia:?}");
    assert!(ia == 1 && q3 == 1 && plan_ia == ["Lunes"]);
}

// ------------------------------------------------------------------ fase 2 con Ollama hostil

/// Respuestas hostiles del modelo en etiquetas, pulido y notas: nada revienta.
#[test]
fn rompedor_reuniones_ollama_hostil_no_revienta_la_fase_2() {
    let mut segs: Vec<Segment> = (0..90)
        .map(|i| seg(1, if i % 2 == 0 { "me" } else { "them" }, i * 4_000, i * 4_000 + 3_000, if i % 3 == 0 { "¿Cerramos el presupuesto el viernes?" } else { "Sí, el viernes lo cerramos con Ana" }))
        .collect();
    let respuestas = [
        "".to_string(),
        "0 r 0\n1 r 1\n2 r 999999999999999999999\n18446744073709551615 p\n91 p\n41 r 40\n5 r 4".into(),
        "{\"turnos\":[{\"n\":-1,\"tipo\":\"pregunta\"},{\"n\":1e300,\"tipo\":\"respuesta\",\"a\":0},{\"n\":\"3\",\"tipo\":\"respuesta\",\"a\":\"2\"}]}".into(),
        "{\"titulo\":null,\"resumen\":[1,2,{\"a\":[]}],\"tareas\":[{\"texto\":\"\\u0000\",\"responsable\":{\"x\":1},\"plazo\":9e999}],\"intercambios\":[{\"pregunta\":\"¿Cerramos el presupuesto el viernes?\",\"pregunto\":\"Ellos\",\"respuesta\":\"Sí\"}]}".into(),
        // Tamaños realistas: `num_predict` 2048 tokens (meeting.rs:624) son ~8-16 KB de texto.
        format!("{{\"resumen\":\"{}\",\"temas\":[{}]}}", "é".repeat(4_000), vec!["\"x\""; 1_000].join(",")),
        "[".repeat(16_000),
        "{".repeat(8_000) + &"}".repeat(8_000),
        "```json\n{\"resumen\": \"ok\"\n```\n{\"resumen\": \"a\"}}}}".into(),
    ];
    let mut fallos = Vec::new();
    for (k, r) in respuestas.iter().enumerate() {
        let r = r.clone();
        let t0 = Instant::now();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut llm = |_: &str, _: f32| -> Result<String, String> { Ok(r.clone()) };
            let _ = notes::polish_turns(&mut llm, &mut segs, None, &mut |_, _| {});
            let _ = notes::label_turns(&mut llm, &mut segs, "Txonso", &mut |_, _| {});
            let _ = notes::understand(&mut llm, &segs, "Txonso", &mut |_, _| {});
            let _ = notes::understand_with(&mut llm, &segs, "Txonso", 0, &mut |_, _| {});
        }));
        eprintln!("respuesta {k} ({} B): {:.1} s{}", r.len(), t0.elapsed().as_secs_f32(), if res.is_err() { " PÁNICO" } else { "" });
        if res.is_err() {
            fallos.push(k);
        }
    }
    assert!(fallos.is_empty(), "pánico con las respuestas {fallos:?}");
}

/// Coste de las notas según cuántos elementos devuelve el modelo en una lista (temas, puntos…).
/// Una respuesta de 2048 tokens puede traer ~1000 elementos cortos.
#[test]
fn rompedor_reuniones_coste_de_listas_largas_del_modelo() {
    let segs: Vec<Segment> = (0..60)
        .map(|i| seg(1, if i % 2 == 0 { "me" } else { "them" }, i * 4_000, i * 4_000 + 3_000, "Hablamos del presupuesto del trimestre y del proveedor nuevo"))
        .collect();
    for n in [125usize, 250, 500, 1_000] {
        let items: Vec<String> = (0..n).map(|i| format!("\"presupuesto del proveedor {i}\"")).collect();
        let r = format!("{{\"resumen\":\"Presupuesto.\",\"temas\":[{l}],\"hechos\":[{l}],\"decisiones\":[{l}]}}", l = items.join(","));
        let mut llm = |_: &str, _: f32| -> Result<String, String> { Ok(r.clone()) };
        let t0 = Instant::now();
        let _ = notes::understand(&mut llm, &segs, "Txonso", &mut |_, _| {});
        eprintln!("{n} elementos por lista ({} B): understand {:.2} s", r.len(), t0.elapsed().as_secs_f32());
    }
}

/// Un modelo pequeño que entra en bucle («jajajaja…» hasta agotar los 2048 tokens) en el resumen:
/// el coste de las notas crece más que linealmente con la longitud de una «palabra» sin espacios.
#[test]
fn rompedor_reuniones_resumen_en_bucle_del_modelo() {
    let segs: Vec<Segment> = (0..60)
        .map(|i| seg(1, if i % 2 == 0 { "me" } else { "them" }, i * 4_000, i * 4_000 + 3_000, "Hablamos del presupuesto del trimestre y del proveedor nuevo"))
        .collect();
    let prosa = |n: usize| "Se habló del presupuesto del trimestre con el proveedor nuevo. ".repeat(n / 62 + 1);
    let casos: Vec<(&str, String)> = vec![
        ("prosa 8 KB", prosa(8_000)),
        ("prosa 16 KB", prosa(16_000)),
        ("jaja 1 KB", "ja".repeat(500)),
        ("jaja 2 KB", "ja".repeat(1_000)),
        ("jaja 4 KB", "ja".repeat(2_000)),
        ("jaja 8 KB", "ja".repeat(4_000)),
        ("é×1000", "é".repeat(1_000)),
        ("é×2000", "é".repeat(2_000)),
        ("é×4000", "é".repeat(4_000)),
        ("jajá×1000 (4 KB)", "jajá".repeat(1_000)),
        ("jajá×2000 (8 KB)", "jajá".repeat(2_000)),
    ];
    for (name, s) in casos {
        let r = format!("{{\"resumen\":\"{s}\",\"temas\":[\"presupuesto\"]}}");
        let mut llm = |_: &str, _: f32| -> Result<String, String> { Ok(r.clone()) };
        let t0 = Instant::now();
        let _ = notes::understand(&mut llm, &segs, "Txonso", &mut |_, _| {});
        eprintln!("{name}: understand {:.2} s", t0.elapsed().as_secs_f32());
    }
}

// ================================================================== segunda pasada (2026-10-23)

/// Una pista sana de `secs` s de habla en `p`.
fn pista_sana(p: &Path, secs: usize) {
    let mut w = WavStream::create_new(p).unwrap();
    w.write(&speech(secs)).unwrap();
    w.finish().unwrap();
}

/// La fase 2 tal como la hacen `meeting.rs:569-597` y `:620-626`, `n` veces («Reintentar»).
fn reintentos(db: &Db, m: &Meeting, n: usize) -> Vec<String> {
    let me = crate::meeting::audio_file(m, "me").unwrap();
    let them = crate::meeting::audio_file(m, "them").unwrap();
    let mut log = Vec::new();
    for intento in 1..=n {
        if db.transcript_final(m.id).unwrap() {
            log.push(format!("intento {intento}: ya final"));
            continue;
        }
        let files = [("them", &them), ("me", &me)];
        let (mut wme, mut wthem, mut failed, mut skipped) = (Vec::new(), Vec::new(), Vec::new(), 0);
        for (who, p) in files {
            match transcribe(p, who) {
                Ok((words, _, s)) => {
                    skipped += s;
                    if who == "me" { wme = words } else { wthem = words }
                }
                Err(e) => failed.push(e),
            }
        }
        let complete = failed.is_empty() && skipped == 0;
        let (segs, _, _) = notes::conversation(wme, wthem, m.id, &|s: &str| Some(s.to_string()));
        db.replace_segments(m.id, &segs, complete).unwrap();
        log.push(format!("intento {intento}: turnos={} fallos={failed:?} final={}", segs.len(), db.transcript_final(m.id).unwrap()));
    }
    log
}

/// Apagón o pantallazo azul a mitad de reunión: NTFS deja a ceros las páginas que estaban en caché
/// sin volcar (los ficheros «llenos de NUL» tras un corte), y la página de la cabecera del WAV es la
/// que más se reescribe (`sync_header` cada 5 s, `audio.rs:318-329`). Las muestras siguen en el
/// disco, pero la pista ya no se lee: falla siempre igual, «Reintentar» nunca da la transcripción
/// por final y la reunión se queda sin notas para siempre (el arreglo de anoche solo cubre ≤ 44 B).
#[test]
fn rompedor_reuniones_2_cabecera_a_ceros_bloquea_las_notas() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Apagón", Some("Zoom"), true, true).unwrap();
    db.finish_meeting(m.id, 120_000).unwrap();
    let me = crate::meeting::audio_file(&m, "me").unwrap();
    let them = crate::meeting::audio_file(&m, "them").unwrap();
    pista_sana(&me, 120);
    pista_sana(&them, 120);
    // La primera página (4 KB) de «Ellos», a ceros: cabecera y 0,13 s de audio.
    let mut b = std::fs::read(&them).unwrap();
    b[..4096].iter_mut().for_each(|x| *x = 0);
    std::fs::write(&them, &b).unwrap();
    let vivas = b[4096..].chunks_exact(2).filter(|c| c != &[0, 0]).count();
    let log = reintentos(&db, &m, 3);
    for l in &log {
        eprintln!("{l}");
    }
    eprintln!("«Ellos»: {} B, {vivas} muestras con audio tras la página a ceros; transcribe_words -> {:?}", b.len(), transcribe(&them, "them").map(|r| r.0.len()));
    // Las dos pistas a ceros en la cabecera: la reunión entera no se puede transcribir nunca.
    let mut b = std::fs::read(&me).unwrap();
    b[..4096].iter_mut().for_each(|x| *x = 0);
    std::fs::write(&me, &b).unwrap();
    eprintln!("las dos pistas: me -> {:?}", transcribe(&me, "me").map(|r| r.0.len()));
    crate::meeting::remove_audio_files(&m);
    assert!(db.transcript_final(m.id).unwrap(), "3 reintentos y la transcripción nunca queda final: sin notas para siempre ({log:?})");
}

// ------------------------------------------------------------------ detección: pestañas y títulos

/// Simula el detector (`meeting.rs:813-840`) cada 3 s con lo que devuelve `world(t)` = (apps con el
/// micro, títulos de ventanas) y aplica Start/Stop como `MeetingManager`. Devuelve las reuniones
/// grabadas `(inicio_s, fin_s, app)`.
fn simular_detector(fin_s: u64, mode: MeetingAuto, world: impl Fn(u64) -> (Vec<(String, bool)>, Vec<(String, String)>)) -> Vec<(u64, u64, String)> {
    let mut m = AutoMachine::new();
    let mut rec = Recording::Idle;
    let mut out: Vec<(u64, u64, String)> = Vec::new();
    let mut t = 0;
    while t < fin_s * 1000 {
        t += 3_000;
        let (mic, titles) = world(t / 1000);
        // La misma cadena que `meeting::spawn_detector`.
        let scan = crate::detect::scan(&mic, &titles);
        let in_use = crate::detect::candidates(&scan, mode == MeetingAuto::Auto);
        match m.step_browsers(t, mode, rec, &in_use, &scan.browsers) {
            Some(AutoAction::Start(app)) => {
                rec = Recording::Auto;
                out.push((t / 1000, fin_s, app));
            }
            Some(AutoAction::Stop) => {
                rec = Recording::Idle;
                if let Some(last) = out.last_mut() {
                    last.1 = t / 1000;
                }
            }
            _ => {}
        }
    }
    out
}

/// Windows solo da el título de la pestaña ACTIVA de cada ventana. Durante una videollamada en el
/// navegador, el usuario mira el correo o un documento en otra pestaña de la misma ventana unos
/// minutos: el título deja de ser el de la llamada, la reunión automática se para sola a los 20 s
/// aunque la llamada siga, ese rato no se graba, y al volver a la pestaña empieza OTRA reunión.
#[test]
fn rompedor_reuniones_2_cambiar_de_pestana_corta_la_reunion() {
    let mut fallos = Vec::new();
    for (navegador, exe, llamada, otra) in [
        ("Edge", "msedge.exe", "Reunión semanal | Microsoft Teams - Trabajo: Microsoft Edge", "Correo: Txonso - Outlook - Trabajo: Microsoft Edge"),
        ("Chrome", "chrome.exe", "Meet - abc-defg-hij - Google Chrome", "Presupuesto 2027 - Hojas de cálculo de Google - Google Chrome"),
    ] {
        // Llamada de 0 a 30 min; de 10:00 a 13:00 el usuario mira otra pestaña en la misma ventana.
        let got = simular_detector(1_800, MeetingAuto::Auto, |s| {
            let mic = vec![(format!("C:#Program Files#{navegador}#{exe}"), true)];
            let title = if (600..780).contains(&s) { otra } else { llamada };
            (mic, vec![(exe.to_string(), title.to_string())])
        });
        let grabado: u64 = got.iter().map(|(a, b, _)| b - a).sum();
        eprintln!("{navegador}: reuniones {got:?}; grabado {grabado} s de 1800 s de llamada");
        if got.len() != 1 {
            fallos.push(format!("{navegador}: {} reuniones, {} s sin grabar", got.len(), 1_800 - grabado - 3));
        }
    }
    assert!(fallos.is_empty(), "{fallos:?}");
}

/// PRD §1 «decidido»: «No graba reuniones de Discord ni de juegos: la detección automática se
/// limita a Teams, Zoom y Meet». El navegador que usa el micro puede tener OTRA ventana con el título
/// propio de Teams sin estar en ninguna llamada (Teams web abierto todo el día en el chat, la página
/// del producto): la llamada de Discord o WhatsApp Web de la otra ventana se graba como Teams.
#[test]
fn rompedor_reuniones_2_otra_ventana_con_teams_graba_discord() {
    let mic = vec![("C:#Program Files#Google#Chrome#Application#chrome.exe".to_string(), true)];
    let casos = [
        ("Discord web + Teams en el chat", "Discord | #general | Amigos - Google Chrome", "Chat | Microsoft Teams - Google Chrome"),
        ("WhatsApp Web + Teams en el calendario", "WhatsApp - Google Chrome", "Calendario | Microsoft Teams - Google Chrome"),
        ("Discord web + página de Teams", "Discord | #juegos - Google Chrome", "Videoconferencias, reuniones y llamadas | Microsoft Teams - Google Chrome"),
        ("Discord web + tutorial en YouTube", "Discord | #general | Amigos - Google Chrome", "Zoom Meeting: tutorial para principiantes - YouTube - Google Chrome"),
        ("WhatsApp Web + artículo", "WhatsApp - Google Chrome", "Jitsi Meet, la alternativa gratis a Zoom - Xataka - Google Chrome"),
    ];
    let mut out = Vec::new();
    for (caso, llamada, otra) in casos {
        let titles = vec![("chrome.exe".to_string(), llamada.to_string()), ("chrome.exe".to_string(), otra.to_string())];
        let got = simular_detector(60, MeetingAuto::Auto, |_| (mic.clone(), titles.clone()));
        eprintln!("{caso}: {got:?}");
        out.push(got);
    }
    // Y por exes: el PRD dice Teams, Zoom y Meet; la lista también graba sola Slack, Webex…
    for exe in ["C:#x#slack.exe", "C:#x#CiscoCollabHost.exe", "C:#x#atmgr.exe"] {
        eprintln!("{exe}: {:?}", crate::detect::detect_all(&[(exe.to_string(), true)], &[]));
    }
    assert!(out.iter().all(|g| g.is_empty()), "{out:?}");
}

/// La automática promete «Se para solo al colgar» (`meeting.rs:175`). Si el usuario pone la
/// detección en «No» con ella en marcha (aunque vuelva a «Automático» un momento después), la
/// máquina olvida que era automática y ya no se para nunca: graba hasta que alguien la pare.
#[test]
fn rompedor_reuniones_2_modo_off_deja_la_automatica_sin_parar() {
    let teams = vec!["Microsoft Teams".to_string()];
    let mut res = Vec::new();
    for vuelve in [false, true] {
        let mut m = AutoMachine::new();
        let mut rec = Recording::Idle;
        let mut stop = None;
        for t in (3_000..7_200_000u64).step_by(3_000) {
            let mode = if t == 30_000 || (t > 30_000 && !vuelve) { MeetingAuto::Off } else { MeetingAuto::Auto };
            let in_use = if t < 600_000 { teams.clone() } else { Vec::new() }; // cuelga a los 10 min
            match m.step(t, mode, rec, &in_use) {
                Some(AutoAction::Start(_)) => rec = Recording::Auto, // `auto_flag` sigue a true
                Some(AutoAction::Stop) => {
                    stop = Some(t / 1000);
                    break;
                }
                _ => {}
            }
        }
        eprintln!("detección a «No» en t=30 s{}: se para sola en {stop:?} s (colgó en 600 s)", if vuelve { " y otra vez «Automático» en t=33 s" } else { "" });
        res.push(stop);
    }
    assert!(res.iter().all(Option::is_some), "{res:?}");
}

// ------------------------------------------------------------------ reparto de turnos: solapes extremos

/// Palabras seguidas desde `t0`, una cada `paso` ms (250 ms de palabra), sin pausas de 700 ms.
fn chorro(t0: i64, n: usize, paso: i64, vocab: &[&str], punto_cada: usize) -> Vec<Word> {
    (0..n)
        .map(|i| {
            let mut text = vocab[i % vocab.len()].to_string();
            if punto_cada > 0 && i % punto_cada == punto_cada - 1 {
                text.push('.');
            }
            Word { start_ms: t0 + i as i64 * paso, end_ms: t0 + i as i64 * paso + 250, text }
        })
        .collect()
}

const OTRO: &[&str] = &["vale", "claro", "entiendo", "perfecto", "exacto", "correcto", "seguro", "genial", "bueno", "listo"];

/// Las dos personas hablan a la vez sin pausas de 700 ms (dos monólogos encima: un vídeo con voz
/// sonando mientras el usuario explica otra cosa, o un intérprete). Tiempo de `conversation()`
/// según la duración: ¿crece lineal?
#[test]
fn rompedor_reuniones_2_solapes_extremos() {
    let mut tiempos = Vec::new();
    for min in [5i64, 10, 20, 40] {
        let n = (min * 60_000 / 330) as usize;
        let them = chorro(0, n, 330, ELLOS, 0);
        let me = chorro(120, n, 330, OTRO, 0);
        let t0 = Instant::now();
        let (segs, _, stats) = notes::conversation(me, them, 1, &|s: &str| Some(s.to_string()));
        let s = t0.elapsed().as_secs_f32();
        eprintln!("{min} min a la vez ({n} palabras por pista, sin puntuación): {} turnos, eco {stats:?}, conversation() {s:.1} s", segs.len());
        tiempos.push((min, s));
    }
    // Con puntuación (Parakeet y Whisper la ponen): frases de 3 palabras.
    for min in [10i64, 20] {
        let n = (min * 60_000 / 330) as usize;
        let t0 = Instant::now();
        let (segs, _, _) = notes::conversation(chorro(120, n, 330, OTRO, 3), chorro(0, n, 330, ELLOS, 3), 1, &|s: &str| Some(s.to_string()));
        eprintln!("{min} min a la vez con punto cada 3 palabras: {} turnos, conversation() {:.1} s", segs.len(), t0.elapsed().as_secs_f32());
    }
    let (m0, s0) = tiempos[0];
    let (m1, s1) = *tiempos.last().unwrap();
    let ratio = (s1 / s0.max(0.001)) / (m1 / m0) as f32;
    eprintln!("crecimiento respecto a lineal: ×{ratio:.1}");
    assert!(ratio < 3.0, "conversation() crece {ratio:.1} veces más que la duración");
}

// ------------------------------------------------------------------ regenerar con tareas hechas

/// «Regenerar» vuelve a pedir las notas al modelo (temperatura 0,1 y 0,5): la misma tarea sale con
/// otras palabras y la marca de hecha se pierde. Solo se conserva con el texto idéntico (sin tildes
/// ni mayúsculas).
#[test]
fn rompedor_reuniones_2_regenerar_con_tareas_parafraseadas() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Daily", None, false, false).unwrap();
    db.finish_meeting(m.id, 60_000).unwrap();
    let task = |t: &str, o: Option<&str>| ParsedAction { text: t.into(), owner: o.map(String::from), due: None };
    let antes = ["Enviar el contrato a Ana", "Llamar al proveedor para confirmar el precio", "Revisar el presupuesto del trimestre"];
    db.save_summary(m.id, &ParsedSummary { summary: "Daily.".into(), action_items: antes.iter().map(|t| task(t, Some("Txonso"))).collect(), ..Default::default() }).unwrap();
    for i in 0..3 {
        db.toggle_action_item(m.id, i).unwrap();
    }
    let despues = ["Enviar contrato a Ana", "Llamar al proveedor y confirmar el precio", "Revisar el presupuesto del trimestre."];
    db.save_summary(m.id, &ParsedSummary { summary: "Daily.".into(), action_items: despues.iter().map(|t| task(t, Some("Txonso"))).collect(), ..Default::default() }).unwrap();
    let d = db.get_meeting_detail(m.id).unwrap().unwrap();
    let marcas: Vec<(String, bool)> = d.action_items.iter().map(|a| (a.text.clone(), a.done)).collect();
    eprintln!("las 3 hechas antes de regenerar; después: {marcas:?}");
    assert!(marcas.iter().all(|(_, d)| *d), "{marcas:?}");
}

// ------------------------------------------------------------------ exportar: Markdown hostil

/// Lo que `md_inert`/`md_block`/`md_line` (summary.rs:611-643) dejan pasar: definiciones de enlace
/// por referencia (`[x]: destino` sin `](`), títulos con subrayado `===`, listas numeradas, casillas
/// de tarea en «Puntos clave» y títulos dentro de un elemento de lista. Deja el Markdown en
/// `PRONTO_ROMPEDOR_MD` (si está) para pasarlo por un renderizador CommonMark.
#[test]
#[ignore = "rompedor: el export deja pasar enlaces por referencia, títulos === y casillas falsas"]
fn rompedor_reuniones_2_exportar_markdown_hostil() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Revisión de pagos", Some("Teams"), false, false).unwrap();
    db.replace_segments(m.id, &[seg(m.id, "them", 0, 3_000, "Revisamos la factura y el pago")], true).unwrap();
    let s = ParsedSummary {
        summary: "Se revisó la factura.\nPagar hoy 5.000 € a ES00 0000\n===\nDetalles en el [pago].\n[pago]: ms-settings:privacy-microphone\n1. Transferir ya".into(),
        key_points: vec!["[x] Transferencia de 5.000 € aprobada".into(), "## Decisiones".into(), "[factura]: //ejemplo.invalid/f".into()],
        decisions: vec!["Pagar la [factura]".into()],
        ..Default::default()
    };
    db.save_summary(m.id, &s).unwrap();
    let md = summary::to_markdown(&db.get_meeting_detail(m.id).unwrap().unwrap());
    eprintln!("{md}");
    if let Ok(p) = std::env::var("PRONTO_ROMPEDOR_MD") {
        std::fs::write(p, &md).unwrap();
    }
    let problemas: Vec<&str> = md
        .lines()
        .filter(|l| *l == "===" || l.starts_with("[pago]:") || l.starts_with("- [x] Transferencia") || l.starts_with("- ## ") || l.starts_with("- [factura]:") || l.starts_with("1. "))
        .collect();
    assert!(problemas.is_empty(), "{problemas:?}");
}

// ------------------------------------------------------------------ Ollama colgado a mitad

/// Ollama contesta la primera petición del pulido y luego se cuelga (otra app le ha encolado una
/// generación larga, la GPU se queda sin memoria…): cada petición espera su plazo entero (240 s,
/// `meeting.rs:660`) y el pulido NO para al primer fallo (`notes.rs:183-187`). Mientras, la reunión
/// está «procesando»: no se puede borrar (`commands.rs:696`), ni reintentar, y el actualizador
/// espera (`updater.rs:903`).
#[test]
fn rompedor_reuniones_2_ollama_colgado_a_mitad_del_pulido() {
    // Una hora de reunión: 400 turnos; uno de cada cuatro, defectuoso para el pulido (gravedad 2:
    // «ritmo», 8 palabras en medio segundo, un trozo mal cortado por el reconocedor).
    let mut segs: Vec<Segment> = (0..400)
        .map(|i| {
            let fin = if i % 4 == 0 { i * 9_000 + 500 } else { i * 9_000 + 8_000 };
            let texto = if i % 2 == 0 { "Revisamos el presupuesto del trimestre con calma esta semana" } else { "Sí, el proveedor sube los precios en enero" };
            seg(1, if i % 2 == 0 { "me" } else { "them" }, i * 9_000, fin, texto)
        })
        .collect();
    let mut llamadas = 0usize;
    let mut fallidas = 0usize;
    let r = {
        let mut llm = |_: &str, _: f32| -> Result<String, String> {
            llamadas += 1;
            if llamadas == 1 {
                Ok("Revisamos el presupuesto del trimestre con calma".into())
            } else {
                fallidas += 1;
                Err("Ollama no respondió en 240 s".into())
            }
        };
        notes::polish_turns(&mut llm, &mut segs, None, &mut |_, _| {})
    };
    let mut llamadas_etq = 0usize;
    let rl = notes::label_turns(
        &mut |_: &str, _: f32| {
            llamadas_etq += 1;
            Err("Ollama no respondió en 240 s".to_string())
        },
        &mut segs,
        "Txonso",
        &mut |_, _| {},
    );
    let mut llamadas_notas = 0usize;
    let ru = notes::understand(
        &mut |_: &str, _: f32| {
            llamadas_notas += 1;
            Err("Ollama no respondió en 240 s".to_string())
        },
        &segs,
        "Txonso",
        &mut |_, _| {},
    );
    let colgadas = fallidas + llamadas_etq + llamadas_notas;
    let espera = 900 + colgadas * 240;
    eprintln!(
        "pulido: {r:?}, {llamadas} llamadas, {fallidas} colgadas; etiquetas: {llamadas_etq} llamadas (err={}); notas: {llamadas_notas} llamadas (err={}); fase 2 con Ollama colgado ≈ 900 s de calentamiento + {colgadas} × 240 s = {:.1} h",
        rl.is_err(),
        ru.is_err(),
        espera as f32 / 3600.0
    );
    assert!(fallidas <= 2, "el pulido sigue llamando a un Ollama colgado: {fallidas} plazos de 240 s");
}

/// Regla 4 con Ollama CAÍDO (conexión rechazada al momento): la transcripción con etiquetas por
/// reglas se guarda y las notas fallan con un motivo claro.
#[test]
fn rompedor_reuniones_2_ollama_caido_la_reunion_se_guarda() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Sin Ollama", None, false, false).unwrap();
    db.finish_meeting(m.id, 60_000).unwrap();
    let mut segs: Vec<Segment> = (0..20)
        .map(|i| seg(m.id, if i % 2 == 0 { "me" } else { "them" }, i * 4_000, i * 4_000 + 3_000, if i % 2 == 0 { "¿Cerramos el presupuesto el viernes?" } else { "Sí, el viernes lo cerramos" }))
        .collect();
    db.replace_segments(m.id, &segs, true).unwrap();
    let mut down = |_: &str, _: f32| -> Result<String, String> { Err("No se pudo contactar con Ollama".into()) };
    let p = notes::polish_turns(&mut down, &mut segs, None, &mut |_, _| {});
    let l = notes::label_turns(&mut down, &mut segs, "Txonso", &mut |_, _| {});
    db.replace_segments(m.id, &segs, true).unwrap();
    let u = notes::understand(&mut down, &segs, "Txonso", &mut |_, _| {});
    let d = db.get_meeting_detail(m.id).unwrap().unwrap();
    let kinds = d.segments.iter().filter(|s| s.kind.is_some()).count();
    eprintln!("pulido {p:?}; etiquetas {l:?}; notas {u:?}; guardados {} turnos, {kinds} con etiqueta por reglas, final={}", d.segments.len(), d.transcript_final);
    assert!(d.segments.len() == 20 && kinds == 20 && d.transcript_final && u.is_err());
}

// ------------------------------------------------------------------ índice de búsqueda con muchas reuniones

/// `meeting_fts` guarda `meeting_id` como UNINDEXED (db.rs:237-242): cada `DELETE … WHERE
/// meeting_id = ?` de `replace_segments` (dos por fase 2) y de `save_summary` recorre el índice
/// entero con el candado de la base cogido (el dictado espera para guardar su historial).
#[test]
fn rompedor_reuniones_2_fts_con_muchas_reuniones() {
    let db = Db::open_memory().unwrap();
    let mut ultima = None;
    for k in 0..300 {
        let m = db.create_meeting_titled(&format!("R{k}"), None, false, false).unwrap();
        let segs: Vec<Segment> = (0..400).map(|i| seg(m.id, "them", i * 4_000, i * 4_000 + 3_000, "Hablamos del presupuesto del trimestre y del proveedor nuevo de Valencia")).collect();
        db.replace_segments(m.id, &segs, true).unwrap();
        ultima = Some((m, segs));
    }
    let (m, segs) = ultima.unwrap();
    let t0 = Instant::now();
    db.replace_segments(m.id, &segs, true).unwrap();
    let t_rep = t0.elapsed();
    let t1 = Instant::now();
    db.save_summary(m.id, &ParsedSummary { summary: "Presupuesto.".into(), ..Default::default() }).unwrap();
    let t_sum = t1.elapsed();
    let t2 = Instant::now();
    let n = db.list_meetings(Some("presu"), 50, 0).unwrap().len();
    eprintln!(
        "120 000 filas en el índice: replace_segments de 400 turnos {:.0} ms; save_summary {:.0} ms; buscar «presu» {:.0} ms ({n})",
        t_rep.as_secs_f32() * 1e3,
        t_sum.as_secs_f32() * 1e3,
        t2.elapsed().as_secs_f32() * 1e3
    );
}

// ------------------------------------------------------------------ manual olvidada / suspensión (coste)

/// Lo que cuesta una hora de silencio en una reunión manual que nadie para (o el hueco que rellena
/// `fill_silence` al volver de una suspensión, `meeting.rs:417-430`): escribirla como hace el hilo
/// de la pista (trozos de 1 s) y transcribirla en la fase 2.
#[test]
fn rompedor_reuniones_2_coste_de_una_hora_de_silencio() {
    let t = Tmp::new("silencio");
    let p = t.join("hora.wav");
    let t0 = Instant::now();
    let mut w = WavStream::create_new(&p).unwrap();
    let zeros = vec![0.0f32; TARGET_RATE as usize];
    for _ in 0..3_600 {
        w.write(&zeros).unwrap();
    }
    w.finish().unwrap();
    let t_w = t0.elapsed();
    let len = std::fs::metadata(&p).unwrap().len();
    let mut llamadas = 0;
    let t1 = Instant::now();
    let r = notes::transcribe_words(
        &p,
        "them",
        &mut |s: &[f32]| {
            llamadas += 1;
            fake_asr(s)
        },
        &mut |_| {},
    );
    eprintln!(
        "1 h de silencio: {:.0} MB por pista, escrita en {:.1} s (perfil test); fase 2: {:.1} s, {llamadas} trozos al motor, {:?} palabras",
        len as f32 / 1e6,
        t_w.as_secs_f32(),
        t1.elapsed().as_secs_f32(),
        r.map(|x| x.0.len())
    );
}

// ------------------------------------------------------------------ reuniones de antes del cambio de nombre

/// Una reunión grabada con la versión instalada (WAV `meeting_{id}_{who}.wav`, sin fecha) que se
/// quedó «grabando» porque Pronto o el PC se cayeron: al arrancar, `close_orphan_meetings`
/// (meetingdb.rs:352-364) le pone `ended_at = started_at`. Tras actualizar, el WAV antiguo solo se
/// acepta si su fecha cae en `[inicio − 1 min, fin + 10 min]` (`meeting.rs:316-317`, db.rs:70-76):
/// pasados 10 min de reunión ya no es «suyo». «Reintentar» no encuentra audio y «Eliminar» deja el
/// WAV en el disco para siempre.
#[test]
#[ignore = "rompedor: una reunión huérfana de la versión anterior pierde su audio tras actualizar"]
fn rompedor_reuniones_2_huerfana_antigua_pierde_su_audio() {
    let db = Db::open_memory().unwrap();
    let m = db.create_meeting_titled("Reunión larga", Some("Teams"), false, false).unwrap();
    // El hilo de la pista escribió 40 min (la última escritura, a los 40 min) y la app se cayó.
    let legacy = crate::db::audio_dir().unwrap().join(format!("meeting_{}_them.wav", m.id));
    pista_sana(&legacy, 8);
    let start = chrono::DateTime::parse_from_rfc3339(&m.started_at).unwrap();
    let mtime: std::time::SystemTime = (start + chrono::Duration::minutes(40)).with_timezone(&chrono::Utc).into();
    std::fs::OpenOptions::new().write(true).open(&legacy).unwrap().set_modified(mtime).unwrap();
    // Arranque siguiente (ya con la versión nueva).
    db.close_orphan_meetings().unwrap();
    let cerrada = db.get_meeting(m.id).unwrap().unwrap();
    // `remove_audio_files` usa el mismo `existing_audio_file` que la fase 2: si no lo borra, la
    // fase 2 tampoco lo ve (final_transcript -> Ok(None) -> «no tiene transcripción»).
    crate::meeting::remove_audio_files(&cerrada);
    let sigue = legacy.is_file();
    eprintln!(
        "ended_at = started_at: {}; WAV de 40 min tras «Eliminar»/«Reintentar»: {}",
        cerrada.ended_at.as_deref() == Some(cerrada.started_at.as_str()),
        if sigue { "no se reconoce como suyo (se queda en el disco y la fase 2 no lo ve)" } else { "reconocido" }
    );
    let _ = std::fs::remove_file(&legacy);
    assert!(!sigue, "el audio de la reunión huérfana no se reconoce");
}

// ------------------------------------------------------------------ regenerar sin Ollama

/// Notas buenas con tareas hechas. El usuario pulsa «Regenerar» (`retry_summary` → estado
/// `pending`, `meeting.rs:752` y `:614`) y: (a) Ollama está caído o el modelo no saca JSON →
/// `fail_summary` (`:731-738`) pone `failed`; (b) cierra Pronto o apaga el PC mientras regenera →
/// al arrancar `close_orphan_meetings` (meetingdb.rs:362) pone `failed`. En los dos casos el
/// resumen, las tareas y sus marcas siguen en la base, pero el hub con `failed` solo pinta «No se
/// pudo generar el resumen» (ui/hub/pages/meetings.js:261-265): las notas buenas desaparecen de la
/// vista (y las tareas no se pueden marcar) hasta que Ollama vuelva a funcionar.
#[test]
fn rompedor_reuniones_2_regenerar_fallido_esconde_las_notas() {
    let db = Db::open_memory().unwrap();
    let mut res = Vec::new();
    for caso in ["Ollama caído al regenerar", "Pronto cerrado mientras regenera"] {
        let m = db.create_meeting_titled("Comité", None, false, false).unwrap();
        db.finish_meeting(m.id, 600_000).unwrap();
        db.replace_segments(m.id, &[seg(m.id, "them", 0, 3_000, "Ana envía el contrato el lunes")], true).unwrap();
        let s = ParsedSummary {
            summary: "Se aprobó el contrato.".into(),
            action_items: vec![ParsedAction { text: "Enviar el contrato".into(), owner: Some("Ana".into()), due: Some("el lunes".into()) }],
            ..Default::default()
        };
        db.save_summary(m.id, &s).unwrap();
        db.toggle_action_item(m.id, 0).unwrap();
        // «Regenerar».
        db.set_summary_status(m.id, "pending").unwrap();
        if caso.starts_with("Ollama") {
            let segs = db.segments(m.id).unwrap();
            let r = notes::understand(&mut |_: &str, _: f32| Err("No se pudo contactar con Ollama".to_string()), &segs, "Txonso", &mut |_, _| {});
            assert!(r.is_err());
            db.fail_summary_status(m.id).unwrap(); // lo que hace `meeting::fail_summary`
        } else {
            db.close_orphan_meetings().unwrap();
        }
        let d = db.get_meeting_detail(m.id).unwrap().unwrap();
        eprintln!(
            "{caso}: estado «{}», en la base: resumen {:?}, tareas {:?} -> el hub pinta solo «No se pudo generar el resumen»",
            d.meeting.summary_status,
            d.summary,
            d.action_items.iter().map(|a| (a.text.as_str(), a.done)).collect::<Vec<_>>()
        );
        res.push(d.meeting.summary_status.clone());
    }
    assert!(res.iter().all(|s| s == "done"), "notas buenas tapadas por un «failed»: {res:?}");
}

/// Dónde se va el tiempo de `rompedor_reuniones_2_solapes_extremos` (10 min a la vez).
#[test]
#[ignore = "rompedor: medida de apoyo de los solapes extremos"]
fn rompedor_reuniones_2_solapes_extremos_desglose() {
    use crate::notes::turns;
    let n = 10 * 60_000 / 330;
    let (mut me, them) = (chorro(120, n, 330, OTRO, 0), chorro(0, n, 330, ELLOS, 0));
    let t = Instant::now();
    turns::remove_echo(&mut me, &them);
    let a = t.elapsed().as_secs_f32();
    let t = Instant::now();
    turns::remove_echo_tails(&mut me, &them);
    let b = t.elapsed().as_secs_f32();
    let mt = turns::split_turns(turns::ME, me, turns::PAUSE_MS);
    let t = Instant::now();
    let mt = turns::drop_echo_turns(mt, &them);
    let c = t.elapsed().as_secs_f32();
    let tt = turns::split_turns(turns::THEM, them, turns::PAUSE_MS);
    let t = Instant::now();
    let conv = turns::join_runs(turns::interleave(mt, tt));
    let d = t.elapsed().as_secs_f32();
    eprintln!("10 min a la vez: remove_echo {a:.1} s, remove_echo_tails {b:.1} s, drop_echo_turns {c:.1} s, interleave+join {d:.2} s ({} turnos)", conv.len());
}
