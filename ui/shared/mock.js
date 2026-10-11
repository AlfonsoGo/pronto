// Mock del backend de Pronto 2 para abrir hub.html en un navegador sin Tauri.
// Simula TODOS los comandos del contrato (docs/ARQUITECTURA.md, «Contrato v2») con datos de ejemplo en
// español y emite dictation://result cada 20 s. Parámetros útiles en la URL (solo con el mock):
//   ?page=insights  ?settings=system  ?theme=light  ?ollama=off  ?empty=1  ?engine=error
//   ?ollama=off (instalado, sin arrancar) ?ollama=none (sin instalar) ?ollama=nomodel (arrancado, sin qwen3:1.7b) ?ai=error (la primera activación falla al 40 %)
//   ?model=missing (tarjeta «Falta el modelo de voz», descarga simulada)  ?model=error (la primera descarga falla al 40 %)
//   ?update=available|event|error|offline|none (aviso de actualización, ver «Contrato v7» en docs/ARQUITECTURA.md)

const qs = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const delay = (ms = 25) => new Promise((r) => setTimeout(r, ms));
const clone = (x) => (x === undefined ? x : JSON.parse(JSON.stringify(x)));

// ------------------------------------------------------------------ bus de eventos
const listeners = new Map();
export function emit(event, payload) {
  (listeners.get(event) || []).slice().forEach((cb) => {
    try { cb({ event, payload: clone(payload) }); } catch (e) { console.error(e); }
  });
}
export async function listen(event, cb) {
  if (!listeners.has(event)) listeners.set(event, []);
  listeners.get(event).push(cb);
  return () => {
    const a = listeners.get(event) || [];
    const i = a.indexOf(cb);
    if (i >= 0) a.splice(i, 1);
  };
}

// ------------------------------------------------------------------ datos
const NOW = new Date();
const pad = (n) => String(n).padStart(2, '0');
const ymd = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
function at(daysAgo, h, m) {
  const d = new Date(NOW);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(h, m, 0, 0);
  if (d > NOW) d.setTime(NOW.getTime() - 60000);
  return d.toISOString();
}
const wordsOf = (t) => (t.trim() ? t.trim().split(/\s+/).length : 0);

let rng = 20261011;
const rand = () => { rng = (rng * 1664525 + 1013904223) % 4294967296; return rng / 4294967296; };

const POOL = [
  ['Hola Marta, te confirmo que la reunión de mañana pasa a las diez y media. Si te viene mal dímelo y la movemos a la tarde.', 'Outlook'],
  ['Perfecto, nos vemos a las ocho entonces. Llevo yo el postre.', 'WhatsApp'],
  ['Revisar el contrato del proveedor antes del viernes, pedir presupuesto actualizado a la imprenta y mandar las fotos de la obra al arquitecto.', 'Notion'],
  ['Esta función recibe la lista de pedidos, filtra los que están pendientes de pago y devuelve el total agrupado por cliente. Hay que añadir una prueba para el caso de lista vacía.', 'Code'],
  ['Gracias por la respuesta tan rápida. Adjunto el documento firmado y quedo pendiente de la factura para tramitar el pago esta semana.', 'Chrome'],
  ['Ayer estuve probando el modelo local y la verdad es que funciona sorprendentemente bien en español, incluso con nombres propios y cifras.', 'Slack'],
  ['Pendientes de la semana. Uno, cerrar la propuesta para el cliente de Bilbao. Dos, preparar la demo del jueves. Tres, ordenar la carpeta de facturas.', 'Word'],
  ['¿Puedes pasarme el enlace de la grabación cuando lo tengas? Quiero revisarla antes de la llamada.', 'Teams'],
  ['No te preocupes, lo miro esta tarde y te cuento.', 'WhatsApp'],
  ['Idea para el vídeo: empezar con el problema, enseñar la solución en treinta segundos y dejar los detalles para el final.', 'Notion'],
  ['He actualizado el diccionario con los nombres de los nuevos productos para que salgan bien escritos.', 'Chrome'],
  ['Buenos días equipo, os recuerdo que hoy cerramos la revisión de la versión dos punto cero. Si encontráis algún fallo, abrid una incidencia con captura y pasos para reproducirlo.', 'Slack'],
  ['Mañana llego sobre las nueve, así que si alguien necesita las llaves que me avise esta noche.', 'Telegram'],
  ['El informe trimestral muestra un aumento del doce por ciento en las altas respecto al trimestre anterior, aunque la retención sigue estable.', 'Word'],
  ['Cuando termine de comer hago la llamada con el proveedor y te escribo el resumen.', 'WhatsApp'],
  ['Comentario para el commit: corrige el cálculo de la racha cuando el último dictado es de ayer y evita contar dos veces el mismo día.', 'Code'],
];

function mkHistory() {
  const spec = [
    [0, 9, 12, 0], [0, 10, 41, 1], [0, 12, 5, 2], [0, 13, 30, 3],
    [1, 8, 55, 4], [1, 11, 20, 5], [1, 16, 2, 6], [1, 18, 47, 7],
    [3, 9, 30, 8], [3, 17, 15, 9], [4, 12, 40, 10], [5, 10, 5, 11],
    [8, 19, 33, 12], [8, 21, 10, 13], [11, 9, 45, 14], [17, 14, 20, 15],
    [22, 10, 0, 0], [26, 18, 30, 4],
  ];
  return spec.map(([d, h, m, p], i) => {
    const [text, app] = POOL[p];
    const words = wordsOf(text);
    return {
      id: 1000 - i, createdAt: at(d, h, m), text, rawText: text.toLowerCase().replace(/[.,¿?]/g, ''),
      app, durationMs: Math.round((words / 2.3) * 1000), words, hasAudio: d < 14,
      ...(i === 3 ? { transform: 'Polish' } : {}),
    };
  });
}

let history = qs.get('empty') ? [] : mkHistory();
let nextHistoryId = 2000;

let words = qs.get('empty') ? [] : [
  { id: 1, word: 'Txonso', replacement: null, createdAt: at(40, 10, 0), starred: true, source: 'manual' },
  { id: 2, word: 'tauri', replacement: 'Tauri', createdAt: at(30, 10, 0), starred: false, source: 'manual' },
  { id: 3, word: 'olama', replacement: 'Ollama', createdAt: at(21, 10, 0), starred: false, source: 'manual' },
  { id: 4, word: 'sherpa onix', replacement: 'sherpa-onnx', createdAt: at(15, 10, 0), starred: true, source: 'manual' },
  { id: 5, word: 'Parakeet', replacement: null, createdAt: at(12, 10, 0), starred: false, source: 'auto' },
  { id: 6, word: 'cubernetes', replacement: 'Kubernetes', createdAt: at(9, 10, 0), starred: false, source: 'manual' },
  { id: 7, word: 'BeltranLabs', replacement: null, createdAt: at(6, 10, 0), starred: false, source: 'auto' },
  { id: 8, word: 'guiti', replacement: 'Git', createdAt: at(2, 10, 0), starred: false, source: 'manual' },
];
let snippets = qs.get('empty') ? [] : [
  { id: 1, trigger: 'mi correo', expansion: 'alfonso@ejemplo.com', createdAt: at(50, 9, 0) },
  { id: 2, trigger: 'firma', expansion: 'Un saludo,\nAlfonso\nBeltranLabs', createdAt: at(40, 9, 0) },
  { id: 3, trigger: 'correo de presentación', expansion: 'Hola, me llamo Alfonso y me encantaría encontrar un momento para conversar sobre cómo podemos ayudarte con tu proyecto. ¿Te viene bien una llamada breve esta semana?', createdAt: at(20, 9, 0) },
  { id: 4, trigger: 'mi linkedin', expansion: 'https://www.linkedin.com/in/alfonso-ejemplo/', createdAt: at(10, 9, 0) },
  { id: 5, trigger: 'dirección de la oficina', expansion: 'Calle Mayor 12, 2.º B, 48001 Bilbao', createdAt: at(3, 9, 0) },
];
let notes = qs.get('empty') ? [] : [
  { id: 1, text: 'Ideas para la demo del jueves\n\n- Empezar mostrando el dictado en el correo\n- Después el diccionario con nombres propios\n- Cerrar con Transforms y Ollama en local', createdAt: at(6, 9, 0), updatedAt: at(1, 10, 0) },
  { id: 2, text: 'Lista de la compra\nCafé en grano, avena, plátanos, pilas AA, papel de cocina.', createdAt: at(8, 9, 0), updatedAt: at(2, 20, 0) },
  { id: 3, text: 'Reunión con el proveedor\n\nAcordamos entrega parcial el día 15 y revisión de precios en noviembre. Pendiente: confirmar por escrito.', createdAt: at(12, 9, 0), updatedAt: at(5, 12, 0) },
  { id: 4, text: 'Frases para el vídeo de presentación: «Tu voz no sale de tu equipo». «Dicta en cualquier app».', createdAt: at(15, 9, 0), updatedAt: at(9, 12, 0) },
  { id: 5, text: 'Ejercicio: lunes y jueves 7:30. Recordar llevar la botella.', createdAt: at(20, 9, 0), updatedAt: at(14, 12, 0) },
  { id: 6, text: 'Preguntas para la llamada\n1. ¿Qué volumen de dictados esperan al día?\n2. ¿Necesitan inglés además de español?\n3. ¿Dónde guardan los audios?', createdAt: at(25, 9, 0), updatedAt: at(19, 12, 0) },
];

const POLISH_PROMPT = 'Reescribe el texto para que sea más claro y conciso. Mantén el mismo idioma, el tono y el significado. Corrige gramática y puntuación. Devuelve solo el texto resultante, sin explicaciones.';
const PE_PROMPT = 'Convierte las ideas desordenadas del texto en un prompt de IA limpio y optimizado: rol, tarea, puntos clave y tono, en secciones breves. Mantén el idioma del texto. Devuelve solo el prompt.';
let transforms = [
  { id: 1, name: 'Polish', prompt: POLISH_PROMPT, builtin: true, hotkeyIndex: 1 },
  { id: 2, name: 'Prompt Engineer', prompt: PE_PROMPT, builtin: true, hotkeyIndex: 2 },
];

const hk = (o) => ({ ctrl: false, alt: false, shift: false, win: false, key: null, ...o });
let settings = {
  hotkeyHold: hk({ win: true, alt: true }),
  hotkeyHandsFree: hk({ win: true, alt: true, key: 'Space' }),
  hotkeyPasteLast: hk({ alt: true, shift: true, key: 'Z' }),
  hotkeyCopyLast: hk({ alt: true, shift: true, key: 'X' }),
  hotkeyTransform: hk({ alt: true, shift: true, key: 'P' }),
  micName: null, language: 'auto', appLanguage: qs.get('lang') === 'en' ? 'en' : 'es',
  modelDir: 'C:\\Users\\Alfonso\\AppData\\Roaming\\Pronto\\Pronto\\models\\parakeet',
  launchAtLogin: true, showFlowBar: true, sounds: true, muteMusic: true,
  removeFillers: true, backtrack: true,
  styles: { personal: 'casual', work: 'casual', email: 'formal', other: 'casual' },
  historyRetentionDays: 0, firstName: 'Alfonso', cleanupLevel: 'medium', pressEnterCommand: false,
  ollamaUrl: 'http://localhost:11434', ollamaModel: 'qwen3:1.7b', keepAudioDays: 14,
  theme: qs.get('theme') === 'light' ? 'light' : 'dark', flowBarHiddenUntil: null,
  autoUpdate: true,
  hotkeyMeeting: hk({ win: true, alt: true, key: 'M' }), meetingAuto: qs.get('auto') === 'ask' ? 'ask' : 'auto', meetingKeepAudio: false,
  aiPolish: qs.get('polish') === 'on', aiPolishModel: 'qwen3:1.7b', aiPolishTimeoutMs: 6000,
};
const DEFAULT_SHORTCUTS = {
  hotkeyHold: clone(settings.hotkeyHold), hotkeyHandsFree: clone(settings.hotkeyHandsFree),
  hotkeyPasteLast: clone(settings.hotkeyPasteLast), hotkeyCopyLast: clone(settings.hotkeyCopyLast),
  hotkeyTransform: clone(settings.hotkeyTransform), hotkeyMeeting: clone(settings.hotkeyMeeting),
};

const modelMissing = qs.get('model') === 'missing' || qs.get('model') === 'error';
let engine = qs.get('engine') === 'error' || modelMissing
  ? { ready: false, loading: false, modelDir: settings.modelDir, error: 'No se encontró encoder.int8.onnx en la carpeta del modelo.' }
  : { ready: false, loading: true, modelDir: settings.modelDir, error: null };
if (engine.loading) setTimeout(() => { engine = { ...engine, ready: true, loading: false }; emit('engine://status', engine); }, 1600);

// Descarga del modelo de voz simulada: ~12 s, con aviso de verificación al final.
const MODEL_FILES = [['encoder.int8.onnx', 652184281], ['decoder.int8.onnx', 11845275], ['joiner.int8.onnx', 6355277], ['tokens.txt', 93939]];
const MODEL_TOTAL = MODEL_FILES.reduce((a, f) => a + f[1], 0);
let model = {
  dir: settings.modelDir, complete: !modelMissing, downloading: false, state: modelMissing ? 'idle' : 'done',
  doneBytes: modelMissing ? 0 : MODEL_TOTAL, totalBytes: MODEL_TOTAL, overallPercent: modelMissing ? 0 : 100, error: null,
  files: MODEL_FILES.map(([name, size]) => ({ name, ok: !modelMissing, bytes: modelMissing ? 0 : size, size })),
};
let modelAttempts = 0;
function modelProgress(state, done, error) {
  const file = state === 'done' ? '' : (MODEL_FILES.find((f, i) => done <= MODEL_FILES.slice(0, i + 1).reduce((a, x) => a + x[1], 0)) || MODEL_FILES[3])[0];
  const p = { file, doneBytes: done, totalBytes: MODEL_TOTAL, overallPercent: Math.floor((done * 100) / MODEL_TOTAL), state };
  if (error) p.error = error;
  model = { ...model, downloading: state === 'downloading' || state === 'verifying', state, doneBytes: done, overallPercent: p.overallPercent, error: error || null };
  emit('model://progress', p);
}
function fakeModelDownload() {
  if (model.downloading) return;
  const failAt = qs.get('model') === 'error' && modelAttempts++ === 0 ? 0.4 : 1;
  let done = 0;
  modelProgress('downloading', 0);
  const tick = setInterval(() => {
    done = Math.min(MODEL_TOTAL, done + MODEL_TOTAL / 60);
    if (done >= MODEL_TOTAL * failAt && failAt < 1) {
      clearInterval(tick);
      modelProgress('error', Math.floor(done), 'encoder.int8.onnx: no se pudo conectar para descargar el modelo: sin conexión');
      return;
    }
    if (done < MODEL_TOTAL) { modelProgress('downloading', Math.floor(done)); return; }
    clearInterval(tick);
    modelProgress('verifying', MODEL_TOTAL);
    setTimeout(() => {
      model = { ...model, complete: true, files: model.files.map((f) => ({ ...f, ok: true, bytes: f.size })) };
      modelProgress('done', MODEL_TOTAL);
      engine = { ...engine, ready: false, loading: true, error: null };
      emit('engine://status', engine);
      setTimeout(() => { engine = { ...engine, ready: true, loading: false }; emit('engine://status', engine); }, 1200);
    }, 900);
  }, 200);
}

// «Activar IA local» simulada (Contrato v6): ?ollama=none recorre las cinco fases (~25 s), ?ollama=off arranca y ya,
// ?ollama=nomodel solo baja el modelo. ?ai=error hace que el primer intento falle al 40 % de la descarga de Ollama.
const OLL_MODE = qs.get('ollama');
const OLL_MODELS = ['qwen2.5:7b', 'qwen3:1.7b', 'llama3.2:3b', 'gemma2:2b'];
const aiSim = {
  installed: OLL_MODE !== 'none', running: !['off', 'none'].includes(OLL_MODE),
  models: OLL_MODE === 'none' ? [] : OLL_MODE === 'nomodel' ? ['llama3.2:3b'] : OLL_MODELS.slice(),
  busy: false, cancel: false, last: null, attempts: 0,
};
const aiEmit = (phase, received = 0, total = 0, message = '') => { aiSim.last = { phase, received, total, message }; emit('ai://progress', aiSim.last); };
const aiStatus = () => {
  const s = settings, modelReady = aiSim.running && [s.ollamaModel, s.aiPolishModel].every((m) => aiSim.models.includes(m));
  const l = aiSim.last;
  return {
    running: aiSim.busy, phase: aiSim.busy || (l && ['error', 'cancelled'].includes(l.phase) && !modelReady) ? l.phase : modelReady ? 'done' : 'idle',
    received: aiSim.busy && l ? l.received : 0, total: aiSim.busy && l ? l.total : 0, message: l && !modelReady ? l.message : '',
    ollamaInstalled: aiSim.installed, ollamaRunning: aiSim.running, model: 'qwen3:1.7b', modelReady, ready: modelReady,
  };
};
async function aiSteps(phase, label, totalBytes, ms, failAt = 1) {
  const n = Math.max(1, Math.round(ms / 150));
  for (let i = 1; i <= n; i++) {
    await delay(150);
    if (aiSim.cancel && phase !== 'install-ollama') throw 'cancel';
    const f = i / n;
    if (f > failAt) throw 'No se pudo conectar para descargar Ollama: sin conexión';
    aiEmit(phase, Math.floor(totalBytes * f), totalBytes, label);
  }
}
async function fakeAiSetup() {
  if (aiSim.busy) return;
  aiSim.busy = true; aiSim.cancel = false;
  const failAt = qs.get('ai') === 'error' && aiSim.attempts++ === 0 ? 0.4 : 1;
  try {
    aiEmit('check', 0, 0, 'Comprobando…'); await delay(500);
    if (!aiSim.running) {
      if (!aiSim.installed) {
        await aiSteps('download-ollama', 'Descargando Ollama…', 1578217944, 9000, failAt);
        aiEmit('install-ollama', 0, 0, 'Comprobando la firma del instalador…'); await delay(1500);
        aiEmit('install-ollama', 0, 0, 'Instalando Ollama…'); await delay(5000);
        aiSim.installed = true;
        if (aiSim.cancel) throw 'cancel';
      }
      aiEmit('start-ollama', 0, 0, 'Arrancando Ollama…'); await delay(2500);
      aiSim.running = true;
    }
    if (![settings.ollamaModel, settings.aiPolishModel].every((m) => aiSim.models.includes(m))) {
      aiEmit('pull-model', 0, 0, 'Preparando la descarga del modelo…'); await delay(700);
      await aiSteps('pull-model', 'Descargando el modelo…', 1429000000, 8000);
      aiEmit('pull-model', 1429000000, 1429000000, 'Verificando el modelo…'); await delay(900);
      if (!aiSim.models.includes('qwen3:1.7b')) aiSim.models.push('qwen3:1.7b');
      settings = { ...settings, ollamaModel: 'qwen3:1.7b', aiPolishModel: 'qwen3:1.7b' };
      emit('settings://changed', settings);
    }
    aiSim.busy = false;
    aiEmit('done', 0, 0, 'IA local lista');
  } catch (e) {
    aiSim.busy = false;
    if (e === 'cancel') aiEmit('cancelled', 0, 0, 'Cancelado'); else aiEmit('error', 0, 0, String(e));
  }
}

// Actualizaciones (Contrato v7). ?update=available (aviso ya visible) · ?update=event (llega a los 3 s, como un
// aviso en segundo plano) · ?update=error (la primera instalación falla al 40 %) · ?update=offline («Buscar ahora» falla).
// Sin parámetro, o con ?update=none, «Buscar ahora» responde «Estás al día».
const UPD = qs.get('update');
const UPD_SIZE = 10_905_600;
const UPD_INFO = {
  version: '2.0.2', tag: 'v2.0.2', title: 'Pronto 2.0.2', pageUrl: 'https://github.com/AlfonsoGo/pronto/releases/tag/v2.0.2',
  publishedAt: new Date(NOW.getTime() - 3600e3).toISOString(), assetName: 'Pronto-Setup-2.0.2.exe', size: UPD_SIZE,
  notes: 'Novedades\n• Actualizaciones automáticas: Pronto avisa cuando hay una versión nueva y se actualiza con un botón.\n• Estilo: el cambio de tono se aplica al pulsar.\n• <b>Arreglo</b>: la bandeja ya no se queda con el aviso <script>alert(1)</script> tras cerrar el hub.\n\nLo demás sigue igual.',
};
let upd = {
  currentVersion: '2.0.1', state: UPD === 'available' || UPD === 'error' ? 'available' : 'idle', checking: false, checkedAt: null,
  info: UPD === 'available' || UPD === 'error' ? clone(UPD_INFO) : null, doneBytes: 0, totalBytes: UPD_SIZE, percent: 0, error: null,
};
if (UPD === 'event') setTimeout(() => { upd = { ...upd, state: 'available', info: clone(UPD_INFO) }; emit('update://available', UPD_INFO); }, 3000);
let updAttempts = 0;
function updProgress(state, done, error) {
  const p = { state, doneBytes: done, totalBytes: UPD_SIZE, percent: Math.floor((done * 100) / UPD_SIZE) };
  if (error) p.error = error;
  upd = { ...upd, state, doneBytes: done, percent: p.percent, error: error || null };
  emit('update://progress', p);
}
function fakeUpdateInstall() {
  if (['downloading', 'verifying', 'installing'].includes(upd.state)) return;
  const failAt = UPD === 'error' && updAttempts++ === 0 ? 0.4 : 1;
  let done = 0;
  updProgress('downloading', 0);
  const tick = setInterval(() => {
    done = Math.min(UPD_SIZE, done + UPD_SIZE / 40);
    if (failAt < 1 && done >= UPD_SIZE * failAt) {
      clearInterval(tick);
      updProgress('error', Math.floor(done), 'El instalador descargado está dañado (el SHA-256 no coincide con el de GitHub). Inténtalo de nuevo.');
      return;
    }
    if (done < UPD_SIZE) { updProgress('downloading', Math.floor(done)); return; }
    clearInterval(tick);
    updProgress('verifying', UPD_SIZE);
    setTimeout(() => {
      updProgress('installing', UPD_SIZE);
      console.info('[mock] Pronto se cerraría ahora; el ayudante instalaría en silencio y lo volvería a abrir.');
    }, 900);
  }, 150);
}

let streakBase = 6;
let totals = { totalWords: 18640, dictations: 412, wordsThisWeek: 2310 };

function mkHeatmap() {
  const out = [];
  for (let i = 364; i >= 0; i--) {
    const d = new Date(NOW); d.setDate(d.getDate() - i);
    const dow = d.getDay();
    let w;
    if (i < streakBase) w = 120 + Math.floor(rand() * 420);
    else if (i === streakBase) w = 0;
    else if (i > 270 && rand() < 0.55) w = 0;
    else if (dow === 0 || dow === 6) w = rand() < 0.6 ? 0 : Math.floor(rand() * 180);
    else w = rand() < 0.14 ? 0 : Math.floor(40 + rand() * 520 * (1 + (365 - i) / 365));
    out.push({ date: ymd(d), words: w });
  }
  return out;
}
const heatmap = mkHeatmap();
const byDay30 = () => heatmap.slice(-30).map((x) => ({ ...x }));

// ------------------------------------------------------------------ audio de ejemplo (WAV 16 kHz)
function wavB64(seconds = 1.4) {
  const sr = 16000, n = Math.floor(sr * seconds);
  const buf = new ArrayBuffer(44 + n * 2);
  const v = new DataView(buf);
  const w = (o, s) => { for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i)); };
  w(0, 'RIFF'); v.setUint32(4, 36 + n * 2, true); w(8, 'WAVE'); w(12, 'fmt ');
  v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
  v.setUint32(24, sr, true); v.setUint32(28, sr * 2, true); v.setUint16(32, 2, true); v.setUint16(34, 16, true);
  w(36, 'data'); v.setUint32(40, n * 2, true);
  for (let i = 0; i < n; i++) {
    const t = i / sr, env = Math.sin((Math.PI * i) / n);
    const s = Math.sin(2 * Math.PI * (220 + 60 * Math.sin(t * 5)) * t) * env * 0.12;
    v.setInt16(44 + i * 2, s * 32767, true);
  }
  let bin = ''; const u = new Uint8Array(buf);
  for (let i = 0; i < u.length; i += 0x8000) bin += String.fromCharCode.apply(null, u.subarray(i, i + 0x8000));
  return btoa(bin);
}

// ------------------------------------------------------------------ dictados nuevos cada 20 s
const FRESH = [
  ['Apunta que mañana hay que llamar al gestor para confirmar la fecha de la entrega.', 'Notion'],
  ['Te paso el resumen de la reunión en cuanto lo tenga limpio, hoy sin falta.', 'Slack'],
  ['Genial, entonces lo dejamos así y cada uno avanza con su parte.', 'WhatsApp'],
  ['Revisa la rama de diseño antes de fusionar, hay un par de estilos que se pisan.', 'Code'],
  ['Estimado cliente, le informamos de que su pedido ha sido enviado y llegará en las próximas cuarenta y ocho horas.', 'Outlook'],
];
let freshIdx = 0;
function fakeDictation() {
  const [text, app] = FRESH[freshIdx++ % FRESH.length];
  const item = {
    id: nextHistoryId++, createdAt: new Date().toISOString(), text, rawText: text.toLowerCase(), app,
    durationMs: Math.round((wordsOf(text) / 2.3) * 1000), words: wordsOf(text), hasAudio: true,
  };
  history.unshift(item);
  totals.totalWords += item.words; totals.dictations += 1; totals.wordsThisWeek += item.words;
  heatmap[heatmap.length - 1].words += item.words;
  emit('dictation://result', item);
}
let timer = null;
function startTimers() {
  if (timer) return;
  timer = setInterval(fakeDictation, 20000);
}

// ------------------------------------------------------------------ insights / stats
function stats() {
  return {
    totalWords: totals.totalWords, wpm: 138.4, streakDays: streakBase, dictations: totals.dictations,
    wordsThisWeek: totals.wordsThisWeek, byDay: byDay30(),
  };
}
function insights() {
  const top = [['Chrome', 5210, 118], ['Code', 3980, 87], ['Slack', 2860, 74], ['WhatsApp', 2240, 61], ['Outlook', 1830, 32],
    ['Notion', 1290, 24], ['Word', 760, 11], ['Teams', 470, 5]];
  return {
    totalWords: totals.totalWords, wpm: 138.4, streakDays: streakBase, longestStreak: 19, dictations: totals.dictations,
    heatmap: clone(heatmap),
    topApps: top.map(([app, w, c]) => ({ app, words: w, count: c })),
    fixes: { dictionary: 214, snippets: 38, fillers: 1276 },
  };
}

// ------------------------------------------------------------------ capture_hotkey (con teclado real del navegador)
function captureHotkey() {
  return new Promise((resolve, reject) => {
    const mods = { ctrl: false, alt: false, shift: false, win: false };
    let key = null;
    const map = (e) => {
      if (e.code === 'Space') return 'Space';
      if (/^Key/.test(e.code)) return e.code.slice(3);
      if (/^Digit/.test(e.code)) return e.code.slice(5);
      return e.key.length === 1 ? e.key.toUpperCase() : e.key;
    };
    const down = (e) => {
      e.preventDefault(); e.stopPropagation();
      if (e.key === 'Escape') { cleanup(); reject('Cancelado'); return; }
      if (e.key === 'Control') mods.ctrl = true;
      else if (e.key === 'Alt') mods.alt = true;
      else if (e.key === 'Shift') mods.shift = true;
      else if (e.key === 'Meta') mods.win = true;
      else key = map(e);
    };
    const up = (e) => {
      e.preventDefault(); e.stopPropagation();
      if (key || mods.ctrl || mods.alt || mods.shift || mods.win) { cleanup(); resolve({ ...mods, key }); }
    };
    const cleanup = () => { window.removeEventListener('keydown', down, true); window.removeEventListener('keyup', up, true); };
    window.addEventListener('keydown', down, true);
    window.addEventListener('keyup', up, true);
  });
}

// ------------------------------------------------------------------ reuniones (Notetaker local)
const mkSegs = (meetingId, script, gap = 11000) => {
  let t = 3000;
  return script.map(([speaker, text]) => {
    const dur = 2500 + text.split(/\s+/).length * 380;
    const seg = { meetingId, speaker, startMs: t, endMs: t + dur, text };
    t += dur + gap * (0.5 + ((text.length * 7) % 10) / 10);
    return seg;
  });
};

const S1 = [
  ['them', 'Buenos días a todos. Empezamos con el estado de la versión dos punto cero, ¿cómo vamos con el motor de voz?'],
  ['me', 'Muy bien. Parakeet funciona de maravilla en español, incluso con nombres propios y cifras. Lo único que falta es probarlo con más micrófonos.'],
  ['them', 'Perfecto. Marta, ¿puedes encargarte de una lista de pruebas con micrófonos USB y auriculares Bluetooth?'],
  ['me', 'Yo me ocupo de la pantalla de estadísticas, tengo el diseño casi cerrado. Lo termino antes del jueves.'],
  ['them', 'Genial. Entonces congelamos las funciones nuevas hasta el viernes y nos centramos en probar.'],
  ['me', 'De acuerdo. Y queda pendiente probar la detección de reuniones en Meet y en Zoom.'],
  ['them', 'Sí, eso lo repartimos al final. ¿Alguna duda sobre Ollama como motor de resúmenes?'],
  ['me', 'Ninguna, para esta versión lo dejamos como único motor. Más adelante vemos si compensa añadir otro.'],
  ['them', 'Perfecto, pues lo dejamos aquí. Gracias a todos.'],
];
const S2 = [
  ['them', 'Hola, te llamo por el presupuesto de las tarjetas. Hemos subido un poco el papel, pero te mantengo el precio si pides mil unidades.'],
  ['me', 'Vale, mil me parece bien. ¿Y para cuándo estarían?'],
  ['them', 'Si me confirmas hoy, las tienes el martes que viene.'],
  ['me', 'Confirmado entonces. Mándame el presupuesto por correo y lo apruebo esta tarde.'],
];
const S3 = [
  ['them', 'Vamos a repasar los objetivos del trimestre. Lo primero es cerrar la versión estable.'],
  ['me', 'Estoy de acuerdo, y añadiría las notas de reunión como segunda prioridad.'],
  ['them', 'Entonces dejamos la sincronización para el trimestre siguiente.'],
];
const S4 = [
  ['them', 'Necesitamos algo para transcribir las visitas a obra sin depender de internet.'],
  ['me', 'Pronto funciona entero en el equipo, así que el audio nunca sale del ordenador.'],
  ['them', 'Eso es justo lo que nos importa. ¿Se puede probar con una licencia de prueba un mes?'],
  ['me', 'Os preparo una instalación de prueba esta misma semana y quedamos para ver cómo va.'],
];
const S5 = [
  ['them', 'Te enseño las pantallas nuevas del panel. He cambiado el mapa de calor y la barra de preguntas.'],
  ['me', 'Me gusta, pero el contraste del tema claro en las etiquetas pequeñas es flojo.'],
  ['them', 'Lo ajusto hoy y te paso una captura cuando esté.'],
];

const MT = (id, title, daysAgo, h, m, mins, app, status, script, extra = {}) => ({
  id, title, startedAt: at(daysAgo, h, m), endedAt: at(daysAgo, h, m), durationMs: mins * 60000,
  app, summaryStatus: status, segments: mkSegs(id, script), summary: null, actionItems: [], decisions: [], topics: [], ...extra,
});
let meetings = qs.get('empty') ? [] : [
  MT(501, 'Revisión semanal de producto', 0, 9, 0, 42, 'Teams', 'done', S1, {
    summary: 'El equipo repasó el estado de la versión 2.0: el motor de voz funciona bien en español y quedan por cerrar la detección de reuniones y el diseño de la pantalla de estadísticas. Se acordó congelar las funciones nuevas hasta el viernes y dedicar el tiempo a probar con distintos micrófonos.',
    actionItems: [
      { text: 'Preparar una lista de pruebas con micrófonos USB y auriculares Bluetooth', owner: 'Marta', done: true },
      { text: 'Cerrar el diseño de la pantalla de estadísticas antes del jueves', owner: 'Alfonso', done: false },
      { text: 'Probar la detección de reuniones en Meet y en Zoom', owner: null, done: false },
    ],
    decisions: ['Congelar las funciones nuevas hasta el viernes', 'Priorizar las pruebas con micrófonos USB y Bluetooth', 'Mantener Ollama como único motor de resúmenes en esta versión'],
    topics: ['Versión 2.0', 'Motor de voz', 'Detección de reuniones', 'Estadísticas'],
  }),
  MT(502, 'Llamada con la imprenta', 0, 10, 30, 18, 'Meet', 'pending', S2),
  MT(503, 'Planificación del trimestre', 1, 16, 0, 55, 'Zoom', 'failed', S3),
  MT(504, 'Reunión con el despacho de arquitectura', 3, 9, 30, 31, 'Meet', 'done', S4, {
    summary: 'El cliente busca transcribir visitas a obra sin depender de internet. Se explicó que Pronto procesa todo en el equipo y se acordó preparar una instalación de prueba de un mes durante esta semana.',
    actionItems: [{ text: 'Preparar la instalación de prueba para el despacho', owner: 'Alfonso', done: false }, { text: 'Agendar una llamada de seguimiento a final de semana', owner: null, done: false }],
    decisions: ['Hacer una prueba de un mes antes de hablar de precio'], topics: ['Privacidad', 'Visitas a obra', 'Instalación de prueba'],
  }),
  MT(505, 'Seguimiento con diseño', 9, 17, 0, 24, 'Teams', 'done', S5, {
    summary: 'Se revisaron las pantallas nuevas del panel. El mapa de calor y la barra de preguntas gustan, pero el contraste de las etiquetas pequeñas en el tema claro necesita ajuste.',
    actionItems: [{ text: 'Ajustar el contraste de las etiquetas pequeñas del tema claro', owner: 'Diseño', done: true }],
    decisions: ['Mantener el nuevo mapa de calor'], topics: ['Panel', 'Tema claro'],
  }),
];

let live = null; // { meeting, idx, timers, startTs }
const LIVE = [
  ['them', 'Buenas, ¿me oís bien? Creo que el micro iba un poco bajo.'],
  ['me', 'Sí, te oímos perfectamente. Cuando quieras empezamos.'],
  ['them', 'Genial. Lo primero es la entrega de la versión nueva: la tenemos prevista para el viernes.'],
  ['me', 'Perfecto. Yo termino hoy las pruebas con auriculares y os las paso mañana por la mañana.'],
  ['them', 'Vale, y Marta se encarga de revisar los textos en inglés antes de publicar.'],
  ['me', 'Anotado. ¿Algo más que debamos decidir hoy?'],
  ['them', 'Sí, si dejamos la sincronización fuera de esta versión. Yo diría que sí.'],
  ['me', 'De acuerdo, la dejamos fuera y la planteamos para el trimestre que viene.'],
];
const SUMMARY_FOR_NEW = {
  summary: 'Se confirmó la entrega de la versión nueva para el viernes. Las pruebas con auriculares se cierran mañana y los textos en inglés pasan por revisión antes de publicar. La sincronización queda fuera de esta versión.',
  actionItems: [
    { text: 'Enviar los resultados de las pruebas con auriculares', owner: 'Alfonso', done: false },
    { text: 'Revisar los textos en inglés antes de publicar', owner: 'Marta', done: false },
  ],
  decisions: ['Entregar la versión nueva el viernes', 'Dejar la sincronización fuera de esta versión'],
  topics: ['Entrega', 'Pruebas', 'Sincronización'],
};
const meetingBase = (m) => { const { segments, summary, actionItems, decisions, topics, ...b } = m; return clone(b); };
const meetingDetail = (m) => clone(m);
const listItem = (m) => ({
  ...meetingBase(m),
  preview: m.summary ? m.summary.split(/(?<=\.)\s/)[0] : (m.segments[0] ? m.segments[0].text : ''),
  actionCount: m.actionItems.length,
});
function finishSummary(m) {
  Object.assign(m, clone(SUMMARY_FOR_NEW), { summaryStatus: 'done' });
  emit('meeting://summary', meetingDetail(m));
  emit('meeting://state', { state: 'done', meetingId: m.id });
}
// la reunión «generando» de ejemplo termina sola a los 25 s
setTimeout(() => {
  const m = meetings.find((x) => x.id === 502);
  if (m && m.summaryStatus === 'pending') {
    Object.assign(m, {
      summary: 'Se acordó mantener el precio de las tarjetas pidiendo mil unidades. El pedido se confirmó el mismo día y llegará el martes siguiente.',
      actionItems: [{ text: 'Aprobar el presupuesto de la imprenta esta tarde', owner: 'Alfonso', done: false }],
      decisions: ['Pedir mil unidades al precio acordado'], topics: ['Presupuesto', 'Plazo de entrega'], summaryStatus: 'done',
    });
    emit('meeting://summary', meetingDetail(m));
  }
}, qs.get('slow') ? 600000 : 25000);

function mdOf(m) {
  const d = new Date(m.startedAt);
  const mm = (ms) => `${pad(Math.floor(ms / 60000))}:${pad(Math.floor((ms % 60000) / 1000))}`;
  let o = `# ${m.title}\n\n${d.toLocaleString('es-ES')} · ${Math.round(m.durationMs / 60000)} min${m.app ? ' · ' + m.app : ''}\n\n`;
  if (m.summary) o += `## Resumen\n\n${m.summary}\n\n`;
  if (m.actionItems.length) o += '## Tareas\n\n' + m.actionItems.map((a) => `- [${a.done ? 'x' : ' '}] ${a.text}${a.owner ? ' (' + a.owner + ')' : ''}`).join('\n') + '\n\n';
  if (m.decisions.length) o += '## Decisiones\n\n' + m.decisions.map((x) => `- ${x}`).join('\n') + '\n\n';
  if (m.topics.length) o += '## Temas\n\n' + m.topics.map((x) => `- ${x}`).join('\n') + '\n\n';
  o += '## Transcripción\n\n' + m.segments.map((s) => `**${s.speaker === 'me' ? 'Tú' : 'Ellos'}** [${mm(s.startMs)}]: ${s.text}`).join('\n\n') + '\n';
  return o;
}

function stopLive() {
  if (!live) return null;
  live.timers.forEach((t) => { clearInterval(t); clearTimeout(t); });
  const m = live.meeting;
  m.endedAt = new Date().toISOString();
  m.durationMs = Date.now() - live.startTs;
  m.summaryStatus = 'pending';
  live = null;
  emit('meeting://state', { state: 'summarizing', meetingId: m.id });
  setTimeout(() => finishSummary(m), qs.get('slow') ? 600000 : 3500);
  return m;
}

const meetingCommands = {
  start_meeting: ({ title, _app, _auto } = {}) => {
    if (live) throw 'Ya hay una reunión en curso.';
    const id = (Date.now() % 100000) + 1000;
    const m = { id, title: title || 'Reunión sin título', startedAt: new Date().toISOString(), endedAt: null, durationMs: 0, app: _app || 'Chrome', auto: !!_auto, summaryStatus: 'none',
      segments: [], summary: null, actionItems: [], decisions: [], topics: [] };
    meetings.push(m);
    live = { meeting: m, startTs: Date.now(), idx: 0, timers: [] };
    emit('meeting://state', { state: 'recording', meetingId: id });
    let lv = [0.3, 0.2];
    const clamp = (x) => Math.max(0, Math.min(1, x));
    live.timers.push(setInterval(() => {
      lv = [clamp(lv[0] + (rand() - 0.5) * 0.6), clamp(lv[1] + (rand() - 0.5) * 0.6)];
      emit('meeting://level', { mic: lv[0], system: lv[1] });
    }, 120));
    const push = () => {
      if (!live) return;
      const [speaker, text] = LIVE[live.idx++ % LIVE.length];
      const startMs = Date.now() - live.startTs;
      const seg = { meetingId: id, speaker, startMs, endMs: startMs + 2500 + text.length * 45, text };
      m.segments.push(seg);
      emit('meeting://segment', seg);
    };
    live.timers.push(setTimeout(push, 1800));
    live.timers.push(setInterval(push, 4800));
    return meetingBase(m);
  },
  stop_meeting: () => { if (!live) throw 'No hay ninguna reunión en curso.'; return meetingBase(stopLive()); },
  meeting_status: () => (live ? { active: true, meeting: meetingBase(live.meeting), elapsedMs: Date.now() - live.startTs } : { active: false, elapsedMs: 0 }),
  list_meetings: ({ query, limit = 50, offset = 0 } = {}) => {
    const q = (query || '').trim().toLowerCase();
    let a = meetings.slice().sort((x, y) => y.startedAt.localeCompare(x.startedAt));
    if (q) a = a.filter((m) => (m.title + ' ' + (m.summary || '') + ' ' + m.segments.map((s) => s.text).join(' ')).toLowerCase().includes(q));
    return a.slice(offset, offset + limit).map(listItem);
  },
  get_meeting: ({ id }) => { const m = meetings.find((x) => x.id === id); if (!m) throw 'La reunión no existe.'; return meetingDetail(m); },
  rename_meeting: ({ id, title }) => { const m = meetings.find((x) => x.id === id); if (m) m.title = String(title).trim() || m.title; },
  delete_meeting: ({ id }) => { if (live && live.meeting.id === id) throw 'Detén la reunión antes de eliminarla.'; meetings = meetings.filter((m) => m.id !== id); },
  retry_summary: ({ id }) => {
    const m = meetings.find((x) => x.id === id); if (!m) throw 'La reunión no existe.';
    m.summaryStatus = 'pending';
    emit('meeting://state', { state: 'summarizing', meetingId: id });
    setTimeout(() => {
      Object.assign(m, {
        summary: 'Se repasaron los objetivos del trimestre: cerrar la versión estable es la primera prioridad y las notas de reunión la segunda. La sincronización entre equipos se aplaza al trimestre siguiente.',
        actionItems: [{ text: 'Cerrar la versión estable antes de fin de mes', owner: 'Alfonso', done: false }, { text: 'Definir el alcance de las notas de reunión', owner: null, done: false }],
        decisions: ['Aplazar la sincronización al trimestre siguiente'], topics: ['Objetivos', 'Versión estable', 'Notas de reunión'], summaryStatus: 'done',
      });
      emit('meeting://summary', meetingDetail(m));
      emit('meeting://state', { state: 'done', meetingId: id });
    }, qs.get('slow') ? 600000 : 3200);
  },
  toggle_action_item: ({ meetingId, index }) => {
    const m = meetings.find((x) => x.id === meetingId); const a = m && m.actionItems[index];
    if (!a) throw 'No existe la tarea.';
    a.done = !a.done;
  },
  export_meeting: ({ id }) => { const m = meetings.find((x) => x.id === id); if (!m) throw 'La reunión no existe.'; return mdOf(m); },
  ask_meetings: async ({ question, meetingId } = {}) => {
    await delay(1500);
    const q = String(question || '').toLowerCase();
    const pool = meetingId != null ? meetings.filter((m) => m.id === meetingId) : meetings.filter((m) => m.summaryStatus === 'done');
    if (!pool.length) return 'Todavía no tengo ninguna reunión con resumen para responderte. Cuando termines una, podrás preguntarme lo que quieras.';
    if (/tarea|pendiente|todo|hacer/.test(q)) {
      const items = pool.flatMap((m) => m.actionItems.filter((a) => !a.done).map((a) => `- ${a.text}${a.owner ? ` (${a.owner})` : ''} · ${m.title}`));
      return items.length ? `Estas son tus tareas pendientes:\n\n${items.join('\n')}` : 'No tienes tareas pendientes en las reuniones con resumen.';
    }
    if (/decid|acord/.test(q)) {
      const d = pool.flatMap((m) => m.decisions.map((x) => `- ${x} · ${m.title}`));
      return d.length ? `Esto es lo que se decidió:\n\n${d.join('\n')}` : 'No encuentro decisiones registradas.';
    }
    const m = pool.slice().sort((a, b) => b.startedAt.localeCompare(a.startedAt))[0];
    return `En «${m.title}» ${m.summary.charAt(0).toLowerCase()}${m.summary.slice(1)}`;
  },
};

// ------------------------------------------------------------------ comandos
const normHist = (q) => (q || '').trim().toLowerCase();

const commands = {
  get_settings: () => clone(settings),
  save_settings: ({ settings: s }) => { settings = { ...settings, ...clone(s) }; emit('settings://changed', settings); },
  list_mics: () => ['Micrófono (Realtek High Definition Audio)', 'Auriculares (Jabra Evolve2 65)', 'Entrada de línea (USB Audio Device)'],
  get_stats: () => stats(),
  list_history: ({ query, limit = 50, offset = 0 }) => {
    const q = normHist(query);
    const all = q ? history.filter((h) => h.text.toLowerCase().includes(q) || h.app.toLowerCase().includes(q)) : history;
    return clone(all.slice(offset, offset + limit));
  },
  delete_history: ({ id }) => { history = history.filter((h) => h.id !== id); },
  clear_history: () => { history = []; },
  copy_text: ({ text }) => { try { navigator.clipboard && navigator.clipboard.writeText(text); } catch (_) { /* sin portapapeles */ } },
  paste_last: () => { if (!history.length) throw 'No hay ninguna transcripción todavía.'; },
  copy_last: () => { if (!history.length) throw 'No hay ninguna transcripción todavía.'; },
  list_words: () => clone(words),
  add_word: ({ word, replacement }) => {
    if (!String(word || '').trim()) throw 'La palabra no puede estar vacía';
    const w = { id: Date.now(), word: word.trim(), replacement: replacement ? replacement.trim() : null, createdAt: new Date().toISOString(), starred: false, source: 'manual' };
    words.push(w); return clone(w);
  },
  update_word: ({ id, word, replacement }) => {
    const w = words.find((x) => x.id === id); if (!w) throw 'No existe';
    w.word = word.trim(); w.replacement = replacement ? replacement.trim() : null;
  },
  delete_word: ({ id }) => { words = words.filter((w) => w.id !== id); },
  toggle_word_star: ({ id }) => { const w = words.find((x) => x.id === id); if (w) w.starred = !w.starred; },
  list_snippets: () => clone(snippets),
  add_snippet: ({ trigger, expansion }) => {
    if (!String(trigger || '').trim() || !expansion) throw 'El disparador y la expansión no pueden estar vacíos';
    const s = { id: Date.now(), trigger: trigger.trim(), expansion, createdAt: new Date().toISOString() };
    snippets.push(s); return clone(s);
  },
  update_snippet: ({ id, trigger, expansion }) => {
    const s = snippets.find((x) => x.id === id); if (!s) throw 'No existe';
    s.trigger = trigger.trim(); s.expansion = expansion;
  },
  delete_snippet: ({ id }) => { snippets = snippets.filter((s) => s.id !== id); },
  list_notes: () => clone(notes.slice().sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))),
  save_note: ({ id, text }) => {
    const now = new Date().toISOString();
    let n = id != null ? notes.find((x) => x.id === id) : null;
    if (n) { n.text = text; n.updatedAt = now; }
    else { n = { id: Date.now(), text, createdAt: now, updatedAt: now }; notes.push(n); }
    return clone(n);
  },
  delete_note: ({ id }) => { notes = notes.filter((n) => n.id !== id); },
  start_dictation: () => {}, stop_dictation: () => {}, cancel_dictation: () => {},
  engine_status: () => clone(engine),
  model_status: () => clone(model),
  model_download_start: () => { fakeModelDownload(); },
  update_status: () => clone(upd),
  update_check: async () => {
    await delay(700);
    if (UPD === 'offline') throw 'No se pudo conectar con GitHub: sin conexión';
    const found = UPD === 'available' || UPD === 'event' || UPD === 'error';
    if (!['downloading', 'verifying', 'installing'].includes(upd.state)) {
      upd = { ...upd, state: found ? 'available' : 'upToDate', info: found ? clone(UPD_INFO) : null };
    }
    upd = { ...upd, checkedAt: new Date().toISOString() };
    if (found) emit('update://available', UPD_INFO);
    return clone(upd);
  },
  update_install: () => { fakeUpdateInstall(); },
  retry_history: ({ id }) => {
    const h = history.find((x) => x.id === id); if (!h) throw 'No existe';
    if (!h.hasAudio) throw 'El audio ya no está disponible.';
    h.text = h.text.replace(/\.$/, '') + '.'; return clone(h);
  },
  get_history_audio: ({ id }) => {
    const h = history.find((x) => x.id === id);
    return h && h.hasAudio ? wavB64(Math.min(3, Math.max(1, h.durationMs / 1000 / 4))) : null;
  },
  list_transforms: () => clone(transforms),
  save_transform: ({ transform }) => {
    const t = clone(transform);
    if (t.id) { const i = transforms.findIndex((x) => x.id === t.id); if (i >= 0) { transforms[i] = { ...transforms[i], ...t }; return clone(transforms[i]); } }
    t.id = Date.now(); t.builtin = false; transforms.push(t); return clone(t);
  },
  delete_transform: ({ id }) => { transforms = transforms.filter((t) => t.id !== id || t.builtin); },
  run_transform: () => {},
  ollama_status: () => (aiSim.running ? { reachable: true, models: aiSim.models.slice() } : { reachable: false, models: [] }),
  ai_setup_start: () => { fakeAiSetup(); },
  ai_setup_cancel: () => { aiSim.cancel = true; },
  ai_setup_status: () => aiStatus(),
  get_insights: () => insights(),
  reset_shortcuts: () => { settings = { ...settings, ...clone(DEFAULT_SHORTCUTS) }; emit('settings://changed', settings); return clone(settings); },
  capture_hotkey: () => captureHotkey(),
  open_hub: () => {}, open_settings: () => {},
  hide_flowbar_for_hour: () => {},
  ...meetingCommands,
};

export async function invoke(cmd, args = {}) {
  await delay(cmd === 'capture_hotkey' ? 0 : 20 + Math.random() * 40);
  const fn = commands[cmd];
  if (!fn) throw `Comando desconocido (mock): ${cmd}`;
  return fn(args || {});
}

// Ventana simulada para los botones de la cabecera.
export function getCurrentWindow() {
  return {
    minimize: async () => console.info('[mock] minimizar'),
    toggleMaximize: async () => console.info('[mock] maximizar/restaurar'),
    hide: async () => console.info('[mock] cerrar = ocultar a la bandeja'),
    isMaximized: async () => false,
  };
}

export function startMock() {
  startTimers();
  // ?auto=1 simula que el backend detecta Teams y empieza solo a los 2,5 s
  if (qs.get('auto') === '1') setTimeout(() => { try { meetingCommands.start_meeting({ title: 'Reunión de Teams', _app: 'Teams', _auto: true }); } catch (_) { /* */ } }, 2500);
}
export const MOCK = true;
