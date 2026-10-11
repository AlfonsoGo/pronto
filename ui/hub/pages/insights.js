// Estadísticas: tiles, mapa de calor anual, barras por app y «Arreglos de Pronto». [spec §2.3 Insights]
import { icon } from '../../shared/icons.js';
import { invoke, listen } from '../api.js';
import { t, esc, $, formatInt, formatDec, formatCompact, getLang, toast, errText } from '../util.js';
import { pageHeader, emptyState } from '../ui.js';

const WORDS_PER_CHAPTER = 3000;
const TYPING_WPM = 40;

function levels(values) {
  const nz = values.filter((v) => v > 0).sort((a, b) => a - b);
  if (!nz.length) return [0, 0, 0];
  const q = (p) => nz[Math.min(nz.length - 1, Math.floor(nz.length * p))];
  return [q(0.25), q(0.5), q(0.8)];
}
const lvl = (v, th) => (v <= 0 ? 0 : v <= th[0] ? 1 : v <= th[1] ? 2 : v <= th[2] ? 3 : 4);

function heatmapHtml(days) {
  if (!days.length) return '';
  const loc = getLang() === 'en' ? 'en-US' : 'es-ES';
  const th = levels(days.map((d) => d.words));
  const first = new Date(days[0].date + 'T00:00:00');
  const pad = (first.getDay() + 6) % 7; // lunes = 0
  const cells = [];
  for (let i = 0; i < pad; i++) cells.push(null);
  days.forEach((d) => cells.push(d));
  const cols = Math.ceil(cells.length / 7);

  // etiquetas de mes: en la columna donde cae el día 1 (o la primera columna)
  const labels = Array(cols).fill('');
  let lastCol = -10;
  cells.forEach((c, i) => {
    if (!c) return;
    const d = new Date(c.date + 'T00:00:00');
    const col = Math.floor(i / 7);
    if ((d.getDate() === 1 || i === pad) && col - lastCol >= 3) {
      labels[col] = d.toLocaleDateString(loc, { month: 'short' }).replace('.', '');
      lastCol = col;
    }
  });
  const dow = [t('lun'), '', t('mié'), '', t('vie'), '', t('dom')];

  return `<div class="hm" style="--cols:${cols}">
    <div class="hm-dow">${dow.map((x) => `<span>${esc(x)}</span>`).join('')}</div>
    <div class="hm-scroll"><div class="hm-wrap">
      <div class="hm-months">${labels.map((l) => `<span>${esc(l)}</span>`).join('')}</div>
      <div class="hm-grid" role="img" aria-label="${esc(t('Mapa de calor de palabras dictadas por día'))}">${cells.map((c) => c
        ? `<i class="hm-c l${lvl(c.words, th)}" data-d="${c.date}" data-w="${c.words}"></i>` : '<i class="hm-c off"></i>').join('')}</div>
    </div></div></div>
    <div class="hm-legend"><span>${esc(t('Menos'))}</span>${[0, 1, 2, 3, 4].map((l) => `<i class="hm-c l${l}"></i>`).join('')}<span>${esc(t('Más'))}</span></div>`;
}

export default {
  id: 'insights',
  mount(root) {
    let alive = true;
    const offs = [];
    root.innerHTML = `<div class="page-inner">${pageHeader(t('Estadísticas'))}<div id="ins-body">${'<div class="skel big"></div>'}</div></div>`;
    const body = $('#ins-body', root);
    const tip = document.createElement('div');
    tip.className = 'hm-tip'; tip.hidden = true;
    document.body.appendChild(tip);

    function render(d) {
      if (!d || !d.dictations) {
        body.innerHTML = emptyState('chart-column', t('Aún no hay estadísticas'), t('Cuando dictes en distintas aplicaciones, aquí verás tus palabras, tu velocidad y tu racha.'));
        return;
      }
      const chapters = Math.floor(d.totalWords / WORDS_PER_CHAPTER);
      const missing = WORDS_PER_CHAPTER - (d.totalWords % WORDS_PER_CHAPTER);
      const factor = d.wpm / TYPING_WPM;
      const last7 = d.heatmap.slice(-7);
      const weekWords = last7.reduce((a, x) => a + x.words, 0);
      const maxApp = Math.max(1, ...d.topApps.map((a) => a.words));
      const topApp = d.topApps[0];
      const dowShort = [t('D'), t('L'), t('M'), t('X'), t('J'), t('V'), t('S')];

      body.innerHTML = `
        <div class="ins-intro"><h2>${esc(t('Le estás dando con todo a Pronto.'))}</h2>
          <p>${esc(t('Aquí tienes una instantánea personal de tu productividad con Pronto.'))}</p></div>

        <div class="ins-tiles">
          <div class="tarj"><div class="in tile">
            <div class="rotulo">${esc(t('Total de palabras dictadas'))}</div>
            <div class="tile-n">${esc(formatInt(d.totalWords))}</div>
            <p class="tile-s">${chapters >= 1
              ? esc(t('¡Has escrito {n} {cap} de un libro!', { n: chapters, cap: chapters === 1 ? t('capítulo') : t('capítulos') }))
              : esc(t('Te faltan {n} palabras para tu primer capítulo.', { n: formatInt(missing) }))}</p>
            <div class="tile-foot"><span class="badge vio">${esc(t('{n} esta semana', { n: formatCompact(weekWords) }))}</span>
              <span class="muted xs">${esc(t('{n} dictados', { n: formatInt(d.dictations) }))}</span></div>
          </div></div>

          <div class="tarj"><div class="in tile">
            <div class="rotulo">${esc(t('Velocidad promedio'))}</div>
            <div class="tile-n">${esc(formatDec(d.wpm, 0))} <small>${esc(t('ppm'))}</small></div>
            <p class="tile-s">${factor >= 1.2
              ? esc(t('Dictas unas {x} veces más rápido de lo que escribes (≈{w} ppm).', { x: formatDec(factor, 1), w: TYPING_WPM }))
              : esc(t('Media de palabras por minuto en todos tus dictados.'))}</p>
            <div class="speedbar" aria-hidden="true"><i style="--w:${Math.min(100, (TYPING_WPM / Math.max(d.wpm, TYPING_WPM)) * 100)}%"></i><i class="b" style="--w:100%"></i></div>
            <div class="speedleg"><span>${esc(t('Teclado'))} ${TYPING_WPM}</span><span>${esc(t('Tu voz'))} ${esc(formatDec(d.wpm, 0))}</span></div>
          </div></div>

          <div class="tarj"><div class="in tile">
            <div class="rotulo">${esc(t('Racha diaria'))}</div>
            <div class="tile-n">${esc(formatInt(d.streakDays))} <small>${esc(d.streakDays === 1 ? t('día') : t('días'))}</small>
              ${d.streakDays > 0 ? `<i class="flame lg">${icon('flame', 22)}</i>` : ''}</div>
            <p class="tile-s">${esc(t('Mejor racha: {n} días', { n: d.longestStreak }))}</p>
            <div class="week" aria-label="${esc(t('Últimos 7 días'))}">${last7.map((x) => {
              const dd = new Date(x.date + 'T00:00:00');
              return `<span class="${x.words > 0 ? 'on' : ''}"><i></i><b>${esc(dowShort[dd.getDay()])}</b></span>`;
            }).join('')}</div>
          </div></div>
        </div>

        <div class="tarj mt"><div class="in">
          <div class="card-hd"><h3>${esc(t('Tu actividad en el último año'))}</h3>
            <span class="muted xs">${esc(t('{n} palabras', { n: formatInt(d.heatmap.reduce((a, x) => a + x.words, 0)) }))}</span></div>
          ${heatmapHtml(d.heatmap)}
          <div class="hm-sum">
            <div><span class="rotulo">${esc(t('Racha actual'))}</span><b>${esc(t('{n} días', { n: d.streakDays }))}</b></div>
            <div><span class="rotulo">${esc(t('Racha más larga'))}</span><b>${esc(t('{n} días', { n: d.longestStreak }))}</b></div>
            <div><span class="rotulo">${esc(t('App favorita'))}</span><b>${topApp ? esc(topApp.app) : '—'}</b></div>
            <div><span class="rotulo">${esc(t('Total de apps usadas'))}</span><b>${d.topApps.length}</b></div>
          </div>
        </div></div>

        <div class="ins-two mt">
          <div class="tarj"><div class="in">
            <div class="card-hd"><h3>${esc(t('Uso por aplicación'))}</h3><span class="rotulo">${esc(t('Total de aplicaciones usadas'))} · ${d.topApps.length}</span></div>
            <ul class="barlist">${d.topApps.map((a, i) => `<li>
              <span class="bl-n">${esc(a.app)}</span>
              <span class="bl-bar"><i style="--w:${Math.round((a.words / maxApp) * 100)}%;--i:${i}"></i></span>
              <span class="bl-v">${esc(formatCompact(a.words))}</span></li>`).join('')}</ul>
          </div></div>

          <div class="tarj"><div class="in">
            <div class="card-hd"><h3>${esc(t('Arreglos de Pronto'))}</h3></div>
            <div class="fixes">
              <div title="${esc(t('Muletillas como «eh», «o sea» y otras correcciones'))}"><b>${esc(formatInt(d.fixes.fillers))}</b><span>${esc(t('palabras corregidas'))}</span><small>${esc(t('Muletillas como «eh», «o sea» y otras'))}</small></div>
              <div title="${esc(t('Correcciones desde tu diccionario'))}"><b>${esc(formatInt(d.fixes.dictionary))}</b><span>${esc(t('correcciones de diccionario'))}</span><small>${esc(t('Nombres y términos escritos a tu manera'))}</small></div>
              <div title="${esc(t('Fragmentos expandidos'))}"><b>${esc(formatInt(d.fixes.snippets))}</b><span>${esc(t('fragmentos expandidos'))}</span><small>${esc(t('Textos largos a una palabra de distancia'))}</small></div>
            </div>
          </div></div>
        </div>`;
    }

    const load = async () => {
      try { const d = await invoke('get_insights'); if (alive) render(d); }
      catch (e) { if (alive) { body.innerHTML = emptyState('triangle-alert', t('Algo salió mal al cargar tus estadísticas.'), errText(e)); } }
    };
    load();

    // tooltip del mapa de calor
    root.addEventListener('mouseover', (e) => {
      const c = e.target.closest('.hm-c[data-d]');
      if (!c) { tip.hidden = true; return; }
      const d = new Date(c.dataset.d + 'T00:00:00');
      const w = Number(c.dataset.w);
      tip.innerHTML = `<b>${esc(w === 0 ? t('Sin dictados') : t('{n} palabras', { n: formatInt(w) }))}</b><span>${esc(d.toLocaleDateString(getLang() === 'en' ? 'en-US' : 'es-ES', { weekday: 'short', day: 'numeric', month: 'long', year: 'numeric' }))}</span>`;
      tip.hidden = false;
      const r = c.getBoundingClientRect();
      const tw = tip.offsetWidth;
      tip.style.left = Math.max(8, Math.min(innerWidth - tw - 8, r.left + r.width / 2 - tw / 2)) + 'px';
      tip.style.top = (r.top - tip.offsetHeight - 8) + 'px';
    });
    root.addEventListener('mouseleave', () => { tip.hidden = true; });
    listen('dictation://result', () => { if (alive) load(); }).then((u) => offs.push(u));

    return () => { alive = false; tip.remove(); offs.forEach((u) => { try { u(); } catch (_) { /* */ } }); };
  },
};
