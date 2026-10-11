// Puente con el backend (Tauri) o con el mock, más el estado compartido del hub.
import * as mock from '../shared/mock.js';
import { setLang, toast, errText, t } from './util.js';

const T = window.__TAURI__;
export const IS_MOCK = !T;

export const invoke = T ? (cmd, args) => T.core.invoke(cmd, args) : (cmd, args) => mock.invoke(cmd, args);

/** listen(evento, cb(payload)) -> Promise<unlisten> */
export const listen = T
  ? (ev, cb) => T.event.listen(ev, (e) => cb(e.payload))
  : (ev, cb) => mock.listen(ev, (e) => cb(e.payload));

export const win = () => {
  try { return T ? T.window.getCurrentWindow() : mock.getCurrentWindow(); } catch (_) { return mock.getCurrentWindow(); }
};

if (IS_MOCK) mock.startMock();

// ------------------------------------------------------------------ estado compartido
export const state = {
  settings: null,
  engine: { ready: false, loading: true, modelDir: '', error: null },
  model: { complete: true, downloading: false, state: 'idle', doneBytes: 0, totalBytes: 0, overallPercent: 0, error: null },
  dictation: { state: 'idle' },
  stats: null,
};
const subs = new Set();
export const subscribe = (fn) => { subs.add(fn); return () => subs.delete(fn); };
export const notify = (what, data) => subs.forEach((fn) => { try { fn(what, data); } catch (e) { console.error(e); } });

export function applySettingsEffects(s) {
  document.documentElement.dataset.theme = s.theme === 'light' ? 'light' : 'dark';
  setLang(s.appLanguage);
}

export async function loadSettings() {
  state.settings = await invoke('get_settings');
  applySettingsEffects(state.settings);
  return state.settings;
}

/** Guarda un cambio parcial: optimista, con reversión si el backend falla. */
export async function saveSettings(patch) {
  const prev = state.settings;
  const next = { ...prev, ...patch };
  state.settings = next;
  applySettingsEffects(next);
  try {
    await invoke('save_settings', { settings: next });
    notify('settings');
  } catch (e) {
    state.settings = prev;
    applySettingsEffects(prev);
    toast(errText(e) || t('No se pudo guardar.'), { kind: 'err' });
    notify('settings');
    throw e;
  }
}

export async function refreshEngine() {
  try { state.engine = await invoke('engine_status'); } catch (_) { /* */ }
  notify('engine');
}

export async function refreshModel() {
  try { state.model = await invoke('model_status'); } catch (_) { /* */ }
  notify('model');
}

/** `model://progress` -> estado compartido. */
export function applyModelProgress(p) {
  if (!p) return;
  const busy = p.state === 'downloading' || p.state === 'verifying';
  state.model = {
    ...state.model, downloading: busy, state: p.state, doneBytes: p.doneBytes, totalBytes: p.totalBytes,
    overallPercent: p.overallPercent, error: p.error || null, complete: state.model.complete || p.state === 'done',
  };
  notify('model');
}

export async function startModelDownload() {
  state.model = { ...state.model, downloading: true, state: 'downloading', error: null };
  notify('model');
  try { await invoke('model_download_start'); }
  catch (e) {
    state.model = { ...state.model, downloading: false, state: 'error', error: errText(e) };
    notify('model');
  }
}

export async function refreshStats() {
  try { state.stats = await invoke('get_stats'); } catch (_) { /* */ }
  notify('stats');
  return state.stats;
}
