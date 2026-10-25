// Novedades de cada versión publicada: la ÚNICA fuente. De aquí salen la página «Novedades» del hub, la tarjeta «Lo nuevo»
// que se enseña una vez tras actualizar y el texto de la release de GitHub (`node scripts/notas-release.mjs`).
// `tests/novedades.test.mjs` falla si la versión de tauri.conf.json no tiene aquí su entrada.
//
// Cómo se escribe (como un changelog de producto, no de commits): castellano llano, el beneficio primero y nada de jerga
// (ni Ollama, ni Flow bar, ni WER). `destacado`: 3-5 cosas que el usuario puede probar ya; `probar` lleva a la pantalla donde
// está ({ page } del hub o { settings } con la pestaña de Configuración). `breve` es la línea del aviso de GitHub (≤ 70 caracteres).
// La 2.1 y la 2.2 no se publicaron: su contenido va dentro de la 2.3.0.

export const WEB = 'pronto.beltranlabs.com';

export const RELEASES = [
  {
    version: '2.3.1', month: '2026-10',
    es: { title: 'Arreglos de la 2.3', intro: 'Una versión pequeña tras revisar la 2.3.0 con usuarios nuevos y pruebas de rotura.' },
    en: { title: '2.3 fixes', intro: 'A small release after reviewing 2.3.0 with new users and break-it tests.' },
    highlights: [
      { icon: 'shield-check',
        probar: { settings: 'data', hint: { es: 'Si eliges conservar menos días, Pronto te pregunta antes de borrar nada.', en: 'If you choose to keep fewer days, Pronto asks before deleting anything.' } },
        es: { t: 'Tus dictados, protegidos', d: 'Si acortas cuánto tiempo se guardan el historial o el audio, Pronto te pregunta antes de borrar nada. Antes, una flecha del teclado de más podía borrar dictados.', breve: 'pregunta antes de borrar historial o audio' },
        en: { t: 'Your dictations, protected', d: 'If you shorten how long history or audio is kept, Pronto asks before deleting anything. Before, one arrow key too many could delete dictations.', breve: 'asks before deleting history or audio' } },
    ],
    improved: [
      { es: 'La IA de tu PC se arranca siempre solo para este PC, nunca abierta a la red.', en: 'The AI on your PC always starts for this PC only, never open to the network.' },
      { es: 'Los avisos de reuniones dicen «Reunión», como el panel.', en: 'Meeting notices say “Meeting”, like the panel.' },
      { es: 'Las novedades también se enseñan a quien solo usa reuniones.', en: 'What’s new is also shown to people who only use meetings.' },
    ],
    fixed: [
      { es: 'Con la IA atascada, cada dictado ya no espera de nuevo antes de pegarse.', en: 'With the AI stuck, each dictation no longer waits again before pasting.' },
      { es: 'Al abrir «Novedades» con el teclado, el foco se queda en su sitio.', en: 'Opening “What’s new” with the keyboard keeps the focus in place.' },
    ],
  },
  {
    version: '2.3.0', month: '2026-10',
    es: { title: 'Más fácil, más rápido y con reuniones de verdad', intro: 'La mayor actualización desde Pronto 2: un panel nuevo, dos motores de voz, un diccionario que aprende cómo hablas y notas de reunión con quién dijo qué.' },
    en: { title: 'Easier, faster, and real meeting notes', intro: 'The biggest update since Pronto 2: a new panel, two speech engines, a dictionary that learns how you speak, and meeting notes with who said what.' },
    highlights: [
      { icon: 'layout-grid',
        probar: { page: 'home', hint: { es: 'Lo de cada día está en el menú de la izquierda; «Cómo se usa», abajo, explica cómo dictar.', en: 'Everyday things are in the left menu; “How to use it”, at the bottom, explains how to dictate.' } },
        es: { t: 'Un panel nuevo, más fácil', d: 'Menú más corto, nombres en castellano llano y más contraste: encuentras cada cosa a la primera.', breve: 'menú más corto y nombres claros' },
        en: { t: 'A new, easier panel', d: 'A shorter menu, plain names and more contrast: you find everything first time.', breve: 'a shorter menu and clear names' } },
      { icon: 'lock',
        probar: { settings: 'system', hint: { es: 'Con la barra en la pantalla que quieras, pulsa el candado que tiene a la derecha.', en: 'With the bar on the screen you want, click the lock on its right.' } },
        es: { t: 'Barra fija con el candado', d: 'Pulsa el candado de la barra y se queda en esa pantalla, aunque dictes en otra.', breve: 'se queda en la pantalla que elijas' },
        en: { t: 'Pin the bar with the lock', d: 'Click the lock on the bar and it stays on that screen, even when you dictate on another one.', breve: 'it stays on the screen you choose' } },
      { icon: 'mic',
        probar: { page: 'dictionary', click: '#page .ph [data-act=add]', spot: '.modal [data-pron=rec]', hint: { es: 'Escribe la palabra y pulsa «Grabar pronunciación» para decirla en voz alta.', en: 'Type the word and click “Record pronunciation” to say it out loud.' } },
        es: { t: 'Enséñale cómo dices una palabra', d: 'En «Mis palabras», graba cómo pronuncias un nombre o una marca y Pronto lo reconoce por tu voz.', breve: 'graba cómo dices un nombre y lo escribe bien' },
        en: { t: 'Teach it how you say a word', d: 'In “My words”, record how you pronounce a name or a brand and Pronto recognises it by your voice.', breve: 'record how you say a name and it gets it right' } },
      { icon: 'disc-2', beta: true,
        probar: { page: 'meetings', hint: { es: 'En tu próxima llamada de Teams, Zoom o Meet, Pronto te preguntará si quieres grabarla.', en: 'On your next Teams, Zoom or Meet call, Pronto will ask whether to record it.' } },
        es: { t: 'Notas de reunión: quién dijo qué', d: 'Al colgar, Pronto separa lo que dijiste tú de lo que dijeron los demás y saca resumen, tareas y decisiones, cada una con la frase exacta y el minuto. La primera vez que detecta una llamada, te pregunta antes de grabarla.', breve: 'resumen, tareas y decisiones con su frase exacta' },
        en: { t: 'Meeting notes: who said what', d: 'When you hang up, Pronto separates what you said from what the others said and writes a summary, tasks and decisions, each with the exact quote and minute. The first time it detects a call, it asks before recording it.', breve: 'summary, tasks and decisions with the exact quote' } },
      { icon: 'cpu',
        probar: { settings: 'engine', hint: { es: 'Elige un motor y pulsa «Usar». Whisper se descarga la primera vez.', en: 'Pick an engine and click “Use”. Whisper downloads the first time.' } },
        es: { t: 'Dos motores de voz', d: 'Además de Parakeet puedes usar Whisper, ahora unas 5 veces más rápido. Cada modelo te dice si es el más rápido o el que menos se equivoca.', breve: 'Parakeet o Whisper, y dictado más rápido' },
        en: { t: 'Two speech engines', d: 'Besides Parakeet you can use Whisper, now about 5 times faster. Each model tells you whether it is the fastest or the most accurate.', breve: 'Parakeet or Whisper, and faster dictation' } },
    ],
    improved: [
      { es: 'Reescribir un texto enseña el resultado antes de pegarlo: Enter para pegarlo, Esc para descartarlo.', en: 'Rewriting a text shows the result before pasting it: Enter to paste, Esc to discard.' },
      { es: 'La IA no inventa: si al pulir aparecen cifras, enlaces o cuentas que no dijiste, se pega tu texto tal cual.', en: 'The AI does not make things up: if polishing adds numbers, links or accounts you did not say, your text is pasted as is.' },
      { es: 'Si la IA de tu PC está instalada pero parada, Pronto la arranca solo.', en: 'If the AI on your PC is installed but stopped, Pronto starts it for you.' },
      { es: 'Copia diaria de tu historial; se guardan las 7 últimas.', en: 'Daily backup of your history; the last 7 are kept.' },
      { es: 'Registro de errores en tu PC, sin nada de lo que dictas, en Configuración → Privacidad y datos.', en: 'An error log on your PC, with nothing you dictate, in Settings → Privacy and data.' },
      { es: 'Los textos largos (600 caracteres o más) se quedan también en el portapapeles.', en: 'Long texts (600 characters or more) also stay on the clipboard.' },
      { es: 'El panel se maneja entero con el teclado y cumple las pautas de accesibilidad WCAG 2.2 AA.', en: 'The panel works fully with the keyboard and meets the WCAG 2.2 AA accessibility guidelines.' },
    ],
    fixed: [
      { es: 'Un dictado que falla ya no se pierde: queda en el historial con su audio para reintentarlo.', en: 'A failed dictation is no longer lost: it stays in the history with its audio so you can retry it.' },
      { es: 'Si la IA de tu PC falla o tarda, el dictado se pega igual, sin pulir.', en: 'If the AI on your PC fails or is slow, the dictation is pasted anyway, unpolished.' },
      { es: 'Una reunión cortada por un apagón recupera su audio.', en: 'A meeting cut off by a power failure gets its audio back.' },
      { es: 'Pronto ya no se actualiza en mitad de un dictado o mientras prepara unas notas.', en: 'Pronto no longer updates in the middle of a dictation or while it prepares meeting notes.' },
      { es: 'Al arrancar con Windows, el historial ya no puede aparecer vacío.', en: 'When starting with Windows, the history can no longer show up empty.' },
      { es: 'Solo puede haber un Pronto abierto a la vez.', en: 'Only one Pronto can be open at a time.' },
      { es: 'El panel ya no se queda en «Cargando el motor…».', en: 'The panel no longer gets stuck on “Loading the engine…”.' },
    ],
  },
  {
    version: '2.0.2', month: '2026-10',
    es: { title: 'IA en tu PC con un botón y actualizaciones solas', intro: '' },
    en: { title: 'On-device AI in one click and automatic updates', intro: '' },
    highlights: [
      { icon: 'sparkles',
        probar: { settings: 'experimental', hint: { es: 'Pulsa «Activar IA local». Antes te dice cuánto ocupa.', en: 'Click “Enable local AI”. It tells you first how much space it takes.' } },
        es: { t: 'IA en tu PC con un botón', d: 'Un botón instala la IA local y su modelo, avisándote antes de lo que ocupa. Sirve para pulir textos y para las notas de reunión.' },
        en: { t: 'On-device AI in one click', d: 'One button installs the local AI and its model, telling you first how much space it takes. It polishes texts and writes meeting notes.' } },
      { icon: 'download',
        probar: { settings: 'system', hint: { es: 'Las actualizaciones automáticas se encienden y apagan en «Actualizaciones».', en: 'Automatic updates are switched on and off under “Updates”.' } },
        es: { t: 'Actualizaciones automáticas', d: 'Pronto busca versiones nuevas al abrirse y cada 4 horas, y se instala con un botón.' },
        en: { t: 'Automatic updates', d: 'Pronto looks for new versions when it opens and every 4 hours, and installs them in one click.' } },
    ],
    improved: [
      { es: 'Un modelo de IA más pequeño y rápido por defecto.', en: 'A smaller, faster AI model by default.' },
    ],
    fixed: [],
  },
  {
    version: '2.0.1', month: '2026-10',
    es: { title: 'Arreglos de la instalación', intro: '' },
    en: { title: 'Installation fixes', intro: '' },
    highlights: [],
    improved: [],
    fixed: [
      { es: 'Ya no aparecen dos «Pronto» instalados.', en: 'You no longer see two “Pronto” installed.' },
      { es: 'Al pulsar un estilo, el cambio se ve al momento.', en: 'When you pick a style, the change shows at once.' },
    ],
  },
  {
    version: '2.0.0', month: '2026-10',
    es: { title: 'Pronto 2: escrito desde cero', intro: 'Pronto se reescribe entero para ser más rápido y ligero, y sigue funcionando todo en tu PC.' },
    en: { title: 'Pronto 2: rewritten from scratch', intro: 'Pronto is rewritten from the ground up to be faster and lighter, and everything still runs on your PC.' },
    highlights: [
      { icon: 'mic', es: { t: 'Dicta en cualquier programa', d: 'Mantén las teclas, habla y suelta: el texto aparece donde está el cursor.' }, en: { t: 'Dictate in any app', d: 'Hold the keys, speak and let go: the text appears where your cursor is.' } },
      { icon: 'disc-2', es: { t: 'Notas de reunión automáticas', d: 'Pronto detecta tus llamadas y toma notas solo.' }, en: { t: 'Automatic meeting notes', d: 'Pronto detects your calls and takes notes on its own.' } },
      { icon: 'shield-check', es: { t: 'Todo en tu PC', d: 'Tu voz y tu texto no salen del equipo. Funciona sin conexión.' }, en: { t: 'Everything on your PC', d: 'Your voice and text never leave your computer. It works offline.' } },
    ],
    improved: [],
    fixed: [],
  },
];

// Clave de cada destacado («2.3.0#2»): la usan los botones «Probar».
for (const r of RELEASES) r.highlights.forEach((h, i) => { h.key = `${r.version}#${i}`; });

/** Primera versión con novedades dentro de la app: quien viene de antes (sin nada guardado) ve desde esta. */
export const FIRST_IN_APP = '2.3.0';
const num = (v) => String(v).split('.').map(Number).reduce((a, x) => a * 1000 + (x || 0), 0);
export const cmpVersion = (a, b) => num(a) - num(b);

/** Versiones con destacados que este usuario no ha visto: de la siguiente a `since` (o desde FIRST_IN_APP) hasta `current`. */
export function pendingReleases(current, since) {
  return RELEASES.filter((r) => r.highlights.length && cmpVersion(r.version, current) <= 0
    && (since && /^\d+\.\d+\.\d+$/.test(since) ? cmpVersion(r.version, since) > 0 : cmpVersion(r.version, FIRST_IN_APP) >= 0));
}

/** Entrada de una versión («2.3.0»), o null. */
export const releaseOf = (version) => RELEASES.find((r) => r.version === version) || null;

/** «Lo nuevo» = destacados de la versión, para la tarjeta que sale una vez tras actualizar. */
export const hasHighlights = (version) => !!(releaseOf(version)?.highlights.length);

/**
 * Cuerpo de la release de GitHub (Markdown). El aviso de actualización de Pronto lo recorta a 600 caracteres y 12 líneas
 * (`updater::plain_notes`) y quita los enlaces: los destacados caben enteros y la web va escrita, no enlazada.
 */
export function githubBody(version) {
  const r = releaseOf(version);
  if (!r) throw new Error(`No hay novedades para la ${version} en ui/hub/novedades.js`);
  const exe = `Pronto-Setup-${r.version}.exe`;
  const lines = [
    `## Pronto ${r.version}`, '',
    `**Descarga:** [${exe}](https://github.com/AlfonsoGo/pronto/releases/download/v${r.version}/${exe}) (Windows 10/11, 64 bits)`, '',
    '### Novedades',
    ...r.highlights.map((h) => `- **${h.es.t}${h.beta ? ' (beta)' : ''}:** ${h.es.breve || h.es.d}.`),
    // Quien actualiza desde una versión anterior a la misma serie recibe también lo de las anteriores (2.3.1 trae la 2.3.0).
    ...RELEASES.filter((o) => o !== r && cmpVersion(o.version, r.version) < 0 && o.version.split('.').slice(0, 2).join('.') === r.version.split('.').slice(0, 2).join('.') && o.highlights.length)
      .map((o) => `- **Incluye la ${o.version}:** ${o.highlights.map((h) => h.es.t.charAt(0).toLowerCase() + h.es.t.slice(1)).join(', ')}.`),
    '',
    `Todo lo nuevo, en Pronto › Novedades y en ${WEB}`,
  ];
  return lines.join('\n') + '\n';
}
