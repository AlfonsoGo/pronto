// Dictado (home): bienvenida, banner, historial por días y columna de estadísticas. [spec §2.3 Dictation]
import { icon } from '../../shared/icons.js';
import { invoke, listen, state, subscribe, refreshModel, startModelDownload } from '../api.js';
import {
  t, esc, $, $$, toast, errText, debounce, formatTime, dayLabel, dayKey, formatCompact, formatInt, formatDec,
  keysHtml, openMenu, confirmDialog,
} from '../util.js';
import { emptyState, heroBanner, waveDeco, searchBtn, searchBar, highlight } from '../ui.js';

const PAGE = 40;
const HERO_KEY = 'pronto.hero.personalization.dismissed';
const lsGet = (k) => { try { return localStorage.getItem(k); } catch (_) { return null; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch (_) { /* */ } };

export default {
  id: 'home',
  mount(root, ctx) {
    let items = [], query = '', hasMore = false, searchOpen = false, loading = false;
    let freshId = null, insights = null, stats = state.stats;
    const expanded = new Set();
    let audio = null, playingId = null;
    const offs = [];
    let alive = true;

    const s = () => state.settings;

    function title() {
      const name = esc(s().firstName || '');
      const total = stats ? stats.totalWords : 0;
      return total >= 2000
        ? t('Hola de nuevo, {name}', { name })
        : t('Hola {name}, vuelve al flow con {keys}', { name, keys: keysHtml(s().hotkeyHold) });
    }

    // ------------------------------------------------------------ esqueleto
    root.innerHTML = `<div class="page-inner">
      <div class="ph"><div class="ph-t"><h1 class="ptitle" id="h-title"></h1></div></div>
      <div class="home">
        <div class="home-main"><div id="h-model"></div><div id="h-hero"></div><div id="h-hist"></div></div>
        <aside class="home-aside" id="h-aside" aria-label="${esc(t('Tu actividad'))}"></aside>
      </div></div>`;
    const elTitle = $('#h-title', root), elHero = $('#h-hero', root), elModel = $('#h-model', root), elHist = $('#h-hist', root), elAside = $('#h-aside', root);

    // ------------------------------------------------------------ modelo de voz
    const mb = (n) => Math.round((Number(n) || 0) / 1048576);
    function renderModel() {
      const m = state.model, e = state.engine;
      if (!m || m.complete || e.ready || e.loading) { elModel.innerHTML = ''; return; }
      let body;
      if (m.downloading) {
        const p = m.overallPercent || 0;
        body = `<div class="mc-prog"><div class="mc-bar" role="progressbar" aria-label="${esc(t('Descarga del modelo de voz'))}" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${p}"><i style="--p:${p}%"></i></div>
          <div class="mc-meta"><b>${p} %</b><span>${esc(m.state === 'verifying' ? t('Verificando la descarga…') : t('{a} de {b} MB', { a: mb(m.doneBytes), b: mb(m.totalBytes) }))}</span></div></div>`;
      } else {
        const failed = m.state === 'error';
        body = `${failed ? `<div class="aviso rojo" role="alert"><div class="in"><span class="ic-l">${icon('triangle-alert', 18)}</span><div class="tx"><b>${esc(t('No se pudo descargar el modelo'))}</b><span>${esc(m.error || '')}</span></div></div></div>` : ''}
          <button class="btn btn-primary" data-act="model-dl">${icon(failed ? 'refresh-cw' : 'download', 16)}${esc(failed ? t('Reintentar') : t('Descargar modelo'))}</button>`;
      }
      elModel.innerHTML = `<div class="model-card tarj"><div class="in">
        <span class="mc-ic" aria-hidden="true">${icon('download', 22)}</span>
        <div class="mc-tx"><h2>${esc(t('Falta el modelo de voz'))}</h2>
          <p>${esc(t('Se descarga una sola vez (≈ 670 MB) y se queda en tu equipo. Después Pronto funciona sin conexión.'))}</p>${body}</div></div></div>`;
    }

    // ------------------------------------------------------------ banner
    function renderHero() {
      if (lsGet(HERO_KEY) !== '1') {
        elHero.innerHTML = heroBanner({
          titleHtml: t('Haz que Pronto suene como <em>tú</em>'),
          body: t('Configura distintos estilos de escritura para distintas apps.'),
          cta: t('Empezar ahora'), ctaAct: 'hero-style', closeAct: 'hero-close', deco: waveDeco(),
        });
      } else {
        elHero.innerHTML = heroBanner({
          titleHtml: t('Mantén presionada {keys} para dictar', { keys: keysHtml(s().hotkeyHold) }),
          body: t('Suelta para pegar el texto en la app con foco. Para dictar sin mantener, pulsa {keys}.', { keys: keysHtml(s().hotkeyHandsFree) }),
          deco: waveDeco(), cls: 'hero-quiet',
        });
      }
    }

    // ------------------------------------------------------------ historial
    function rowHtml(it) {
      const clamp = expanded.has(it.id) ? '' : ' clamp';
      const dur = it.durationMs ? Math.max(1, Math.round(it.durationMs / 1000)) : 0;
      return `<article class="hrow${it.id === freshId ? ' fresh' : ''}" data-id="${it.id}">
        <div class="h-time"><span>${esc(formatTime(it.createdAt))}</span>${it.app ? `<small>${esc(it.app)}</small>` : ''}</div>
        <div class="h-body">
          <p class="h-text${clamp}" data-act="expand" tabindex="0">${highlight(it.text || '', query) || `<span class="muted">${esc(t('La transcripción está vacía'))}</span>`}</p>
          <div class="h-meta">${it.transform ? `<span class="badge vio">${icon('wand-sparkles', 12)}${esc(it.transform)}</span>` : ''}
            <span>${esc(t('{n} palabras', { n: it.words ?? 0 }))}${dur ? ' · ' + dur + ' s' : ''}</span></div>
        </div>
        <div class="h-acts">
          <button class="icon-btn" data-act="copy" aria-label="${esc(t('Copiar transcripción'))}" title="${esc(t('Copiar transcripción'))}">${icon('copy', 17)}</button>
          <button class="icon-btn" data-act="play" ${it.hasAudio ? '' : 'disabled'} aria-label="${esc(t('Reproducir audio'))}"
            title="${esc(it.hasAudio ? t('Reproducir audio') : t('El audio ya no está disponible (más de 14 días)'))}">${icon(playingId === it.id ? 'pause' : 'play', 16)}</button>
          <button class="icon-btn" data-act="more" aria-label="${esc(t('Más opciones'))}" title="${esc(t('Más opciones'))}">${icon('ellipsis', 18)}</button>
        </div></article>`;
    }

    function renderHist() {
      if (loading && !items.length) { elHist.innerHTML = `<div class="skel-list">${'<div class="skel"></div>'.repeat(4)}</div>`; return; }
      const bar = searchOpen ? searchBar({ placeholder: t('Buscar transcripciones anteriores'), value: query }) : '';
      if (!items.length) {
        elHist.innerHTML = (searchOpen ? bar : '') + (query
          ? emptyState('search', t('No se encontraron transcripciones para "{q}"', { q: query }), '')
          : emptyState('mic', t('Aún no hay dictados'), t('Cuando dictes en distintas aplicaciones, todo tu historial de dictado aparecerá aquí.')));
        keepSearchFocus();
        return;
      }
      let html = bar, group = '', first = true;
      for (const it of items) {
        const k = dayKey(it.createdAt);
        if (k !== group) {
          if (group) html += '</div></section>';
          group = k;
          html += `<section class="day"><div class="day-hd"><span class="overline">${esc(dayLabel(it.createdAt))}</span>${first && !searchOpen ? searchBtn() : ''}</div><div class="lista">`;
          first = false;
        }
        html += rowHtml(it);
      }
      html += '</div></section>';
      if (hasMore) html += `<div class="more-wrap"><button class="btn btn-secondary" data-act="load-more">${esc(t('Cargar más'))}</button></div>`;
      elHist.innerHTML = html;
      keepSearchFocus();
    }
    function keepSearchFocus() {
      if (!searchOpen) return;
      const inp = $('[data-search-input]', elHist);
      if (inp && searchWantsFocus) { inp.focus(); inp.setSelectionRange(inp.value.length, inp.value.length); }
    }
    let searchWantsFocus = false;

    async function load(reset = true) {
      loading = true;
      if (reset) { items = []; renderHist(); }
      try {
        const offset = reset ? 0 : items.length;
        const page = await invoke('list_history', { query: query || null, limit: PAGE, offset });
        if (!alive) return;
        items = reset ? page : items.concat(page);
        hasMore = page.length === PAGE;
      } catch (e) { toast(errText(e), { kind: 'err' }); }
      loading = false;
      renderHist();
    }
    const doSearch = debounce(() => load(true), 250);

    // ------------------------------------------------------------ aside
    function renderAside() {
      const st = stats || { totalWords: 0, wpm: 0, streakDays: 0, wordsThisWeek: 0 };
      const top = (insights && insights.topApps ? insights.topApps : []).slice(0, 4);
      const max = Math.max(1, ...top.map((a) => a.words));
      elAside.innerHTML = `
        <div class="tarj"><div class="in stats">
          <div class="stat"><b>${esc(formatCompact(st.totalWords))}</b><span>${esc(t('palabras totales'))}</span></div>
          <div class="stat"><b>${esc(formatDec(st.wpm, 0))}</b><span>${esc(t('ppm'))}</span></div>
          <div class="stat"><b>${esc(formatInt(st.streakDays))}</b><span>${esc(t('días seguidos'))}</span>${st.streakDays > 0 ? `<i class="flame">${icon('flame', 15)}</i>` : ''}</div>
          ${st.wordsThisWeek ? `<div class="stat-foot">${esc(t('{n} palabras esta semana', { n: formatInt(st.wordsThisWeek) }))}</div>` : ''}
        </div></div>
        <div class="tarj apps-card"><div class="in">
          <h3>${esc(t('Vuelve a tu Flow'))}</h3>
          ${top.length ? `<p class="muted sm">${esc(t('Tus apps más usadas'))}</p><ul class="toplist">${top.map((a) =>
            `<li><span class="tl-n">${esc(a.app)}</span><span class="tl-v">${esc(formatCompact(a.words))}</span><i class="tl-bar" style="--w:${Math.round((a.words / max) * 100)}%"></i></li>`).join('')}</ul>`
            : `<p class="muted sm">${esc(t('Empieza a usar Pronto en cualquier app para ver aquí tus apps más usadas.'))}</p>`}
        </div></div>`;
    }

    // ------------------------------------------------------------ acciones
    async function playRow(id, btn) {
      if (audio && playingId === id) { audio.pause(); stopAudio(); return; }
      stopAudio();
      btn.disabled = true;
      try {
        const b64 = await invoke('get_history_audio', { id });
        if (!b64) { toast(t('No se puede reproducir el audio porque ya no está disponible (tiene más de 14 días)'), { kind: 'err' }); btn.disabled = false; return; }
        audio = new Audio('data:audio/wav;base64,' + b64);
        playingId = id;
        audio.addEventListener('ended', stopAudio);
        audio.addEventListener('error', () => { stopAudio(); toast(t('No se pudo reproducir el audio'), { kind: 'err' }); });
        await audio.play();
        setPlayIcon(id, true);
      } catch (e) { stopAudio(); toast(errText(e), { kind: 'err' }); }
      btn.disabled = false;
    }
    function setPlayIcon(id, playing) {
      const b = $(`.hrow[data-id="${id}"] [data-act=play]`, elHist);
      if (b) b.innerHTML = icon(playing ? 'pause' : 'play', 16);
    }
    function stopAudio() {
      const id = playingId;
      if (audio) { try { audio.pause(); } catch (_) { /* */ } audio = null; }
      playingId = null;
      if (id != null) setPlayIcon(id, false);
    }

    async function retry(it) {
      toast(t('Reintentando la transcripción…'), { kind: 'info' });
      try {
        const n = await invoke('retry_history', { id: it.id });
        const i = items.findIndex((x) => x.id === it.id);
        if (i >= 0) items[i] = { ...items[i], ...n };
        renderHist();
        toast(t('Transcripción actualizada'));
      } catch (e) { toast(errText(e) || t('Falló el reintento. Inténtalo de nuevo.'), { kind: 'err' }); }
    }
    async function remove(it) {
      const ok = await confirmDialog({
        title: t('¿Seguro que quieres eliminar esta transcripción?'),
        body: t('Una vez eliminado, no se puede recuperar'),
      });
      if (!ok) return;
      try {
        await invoke('delete_history', { id: it.id });
        if (playingId === it.id) stopAudio();
        items = items.filter((x) => x.id !== it.id);
        renderHist();
        toast(t('Transcripción eliminada'));
        refreshAside();
      } catch (e) { toast(errText(e), { kind: 'err' }); }
    }

    root.addEventListener('click', async (e) => {
      const b = e.target.closest('[data-act]');
      if (!b) return;
      const act = b.dataset.act;
      const row = b.closest('.hrow');
      const it = row ? items.find((x) => x.id === Number(row.dataset.id)) : null;
      if (act === 'model-dl') startModelDownload();
      else if (act === 'hero-style') ctx.navigate('style');
      else if (act === 'hero-close') { lsSet(HERO_KEY, '1'); renderHero(); }
      else if (act === 'toggle-search') { searchOpen = true; searchWantsFocus = true; renderHist(); }
      else if (act === 'clear-search') {
        searchOpen = false; searchWantsFocus = false;
        if (query) { query = ''; load(true); } else renderHist();
      } else if (act === 'load-more') load(false);
      else if (act === 'expand' && it) {
        const sel = window.getSelection && window.getSelection().toString();
        if (sel) return;
        expanded.has(it.id) ? expanded.delete(it.id) : expanded.add(it.id);
        b.classList.toggle('clamp', !expanded.has(it.id));
      } else if (act === 'copy' && it) {
        if (!it.text) { toast(t('La transcripción está vacía'), { kind: 'err' }); return; }
        try { await invoke('copy_text', { text: it.text }); toast(t('Transcripción copiada')); }
        catch (err) { toast(errText(err) || t('No se pudo copiar la transcripción. Inténtalo de nuevo.'), { kind: 'err' }); }
      } else if (act === 'play' && it) playRow(it.id, b);
      else if (act === 'more' && it) {
        openMenu(b, [
          { label: t('Reintentar transcripción'), icon: 'rotate-ccw', disabled: !it.hasAudio, onClick: () => retry(it) },
          { label: t('Eliminar transcripción'), icon: 'trash-2', danger: true, onClick: () => remove(it) },
        ]);
      }
    });
    root.addEventListener('keydown', (e) => {
      if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('.h-text')) { e.preventDefault(); e.target.click(); }
    });
    root.addEventListener('input', (e) => {
      if (e.target.matches('[data-search-input]')) { query = e.target.value.trim(); searchWantsFocus = true; doSearch(); }
    });
    root.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && e.target.matches('[data-search-input]')) { e.stopPropagation(); $('[data-act=clear-search]', root).click(); }
    });

    // ------------------------------------------------------------ datos y eventos
    async function refreshAside() {
      try {
        const [st, ins] = await Promise.all([invoke('get_stats'), invoke('get_insights')]);
        stats = st; insights = ins; state.stats = st;
      } catch (_) { /* */ }
      if (!alive) return;
      elTitle.innerHTML = title();
      renderAside();
    }

    (async () => {
      elTitle.innerHTML = title();
      renderHero(); renderAside(); renderModel();
      refreshModel();
      loading = true; renderHist();
      await Promise.all([load(true), refreshAside()]);
    })();

    listen('dictation://result', (it) => {
      if (!alive || !it) return;
      refreshAside();
      if (query) return;
      freshId = it.id;
      items = [it, ...items.filter((x) => x.id !== it.id)];
      renderHist();
      toast(t('Nuevo dictado: {n} palabras', { n: it.words ?? 0 }), { kind: 'info', ms: 2000 });
      setTimeout(() => { freshId = null; }, 1500);
    }).then((u) => offs.push(u));
    listen('settings://changed', () => { if (alive) { elTitle.innerHTML = title(); renderHero(); } }).then((u) => offs.push(u));

    offs.push(subscribe((what) => { if (alive && (what === 'model' || what === 'engine')) renderModel(); }));

    return () => { alive = false; stopAudio(); offs.forEach((u) => { try { u(); } catch (_) { /* */ } }); doSearch.cancel(); };
  },
};
