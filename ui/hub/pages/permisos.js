// Pronto 2 · Permisos de macOS: qué necesita Pronto en el Mac, para qué, y un botón que lleva al sitio
// exacto de Ajustes. Solo sale en Mac (en Windows no hay permisos que dar). Se refresca sola cada 2 s
// mientras está abierta, así la persona ve el «Listo» en cuanto activa el interruptor en Ajustes.
import { icon } from '../../shared/icons.js';
import { invoke } from '../api.js';
import { t, esc, toast, errText } from '../util.js';
import { pageHeader } from '../ui.js';

const ITEMS = [
  {
    id: 'accessibility',
    title: () => t('Accesibilidad'),
    why: () => t('Para pegar el texto donde estás escribiendo (Pronto pulsa ⌘V por ti) y para oír el atajo de dictado.'),
    how: () => t('En Ajustes, activa el interruptor de Pronto. Si no aparece, pulsa + y elige Pronto en Aplicaciones.'),
    ok: (p) => p.accessibility,
  },
  {
    id: 'inputMonitoring',
    title: () => t('Monitorización de entrada'),
    why: () => t('Algunos Mac la piden aparte para oír la tecla fn mientras usas otras apps. Pronto no guarda lo que escribes.'),
    how: () => t('En Ajustes, activa el interruptor de Pronto. Si te pide salir y volver a abrir Pronto, hazlo.'),
    ok: (p) => p.inputMonitoring || p.keyboardHook,
  },
  {
    id: 'microphone',
    title: () => t('Micrófono'),
    why: () => t('Para escucharte mientras dictas o grabas una reunión. La voz se convierte en texto en tu Mac y nunca sale de él.'),
    how: () => t('Pulsa Permitir. Si antes dijiste que no, actívalo en Ajustes.'),
    ok: (p) => p.microphone === 'granted',
  },
  {
    id: 'keyboard',
    title: () => t('Tecla fn (🌐)'),
    why: () => t('Pronto dicta mientras mantienes fn. Si la tecla 🌐 abre los emojis o el dictado de Apple, saldrían las dos cosas a la vez.'),
    how: () => t('En Ajustes > Teclado, en «Pulsar la tecla 🌐 para», elige «No hacer nada».'),
    ok: (p) => !p.globeKeyConflict,
  },
];

let timer = 0;

function itemHtml(it, p) {
  const ok = !!p && it.ok(p);
  const btn = it.id === 'microphone' && p && p.microphone === 'ask' ? t('Permitir') : t('Abrir Ajustes');
  return `<section class="tarj perm" aria-labelledby="perm-${it.id}"><div class="in">
    <div class="perm-h"><h2 id="perm-${it.id}">${esc(it.title())}</h2>
      <span class="badge ${ok ? 'ok' : 'warn'}" role="status">${ok ? esc(t('Listo')) : esc(t('Falta'))}</span></div>
    <p>${esc(it.why())}</p>
    ${ok ? '' : `<p class="perm-how">${esc(it.how())}</p>
    <button class="btn btn-primary btn-sm" data-perm="${it.id}">${icon('arrow-up-right', 16)}${esc(btn)}</button>`}
  </div></section>`;
}

function render(root, info) {
  const p = info && info.permissions;
  const all = p && p.allOk;
  root.innerHTML = `<div class="page-inner perm-page">
    ${pageHeader(t('Permisos del Mac'), { sub: esc(all ? t('Todo listo: mantén fn y habla.') : t('macOS pide tu permiso para que Pronto pueda escucharte y escribir por ti. Todo se queda en tu Mac.')) })}
    ${ITEMS.map((it) => itemHtml(it, p)).join('')}
    <p class="perm-foot">${esc(t('Pronto no envía tu voz ni tu texto a ningún sitio. Estos permisos solo dejan que funcione dentro de tu Mac.'))}</p>
  </div>`;
}

export default {
  id: 'permisos',
  mount(root) {
    let alive = true;
    const refresh = async () => {
      try {
        const info = await invoke('platform_info');
        if (alive) render(root, info);
      } catch (e) {
        if (alive) root.innerHTML = `<div class="page-inner">${pageHeader(t('Permisos del Mac'), { sub: esc(errText(e)) })}</div>`;
      }
    };
    const onClick = async (e) => {
      const b = e.target.closest('[data-perm]');
      if (!b) return;
      try { await invoke('request_permission', { kind: b.dataset.perm }); } catch (err) { toast(errText(err), { kind: 'err' }); }
      setTimeout(refresh, 800);
    };
    root.addEventListener('click', onClick);
    refresh();
    clearInterval(timer);
    timer = setInterval(refresh, 2000);
    return () => { alive = false; clearInterval(timer); root.removeEventListener('click', onClick); };
  },
};
