// Estado global de la reunión en curso (Notetaker local). Vive fuera de las páginas para que la
// reunión siga «viva» en el sidebar aunque el usuario cambie de sección.
import { invoke, listen, notify } from './api.js';

export const live = {
  active: false,
  meeting: null,          // Meeting en curso
  startedAt: 0,           // Date.now() - elapsedMs
  segments: [],           // segmentos recibidos en vivo
  level: { mic: 0, system: 0 },
  summarizing: new Set(), // ids con resumen pendiente
  auto: false,            // reunión iniciada sola (detección), no desde el botón del hub
  manual: false,          // el hub acaba de pedir start_meeting
};

export const expectManual = (v = true) => { live.manual = v; };

export const elapsedMs = () => (live.active ? Math.max(0, Date.now() - live.startedAt) : 0);

export function formatClock(ms) {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), x = s % 60;
  const p = (n) => String(n).padStart(2, '0');
  return h ? `${h}:${p(m)}:${p(x)}` : `${p(m)}:${p(x)}`;
}

export function formatDuration(ms) {
  const m = Math.round(ms / 60000);
  if (m < 1) return '<1 min';
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60), r = m % 60;
  return r ? `${h} h ${r} min` : `${h} h`;
}

function setActive(meeting, elapsed = 0) {
  live.active = true;
  // El backend dice quién la arrancó (`Meeting.auto`): la detección o la persona.
  live.auto = !!(meeting && meeting.auto);
  live.meeting = meeting;
  live.startedAt = Date.now() - elapsed;
  live.segments = [];
  live.level = { mic: 0, system: 0 };
}
function clearActive() {
  live.active = false;
  live.level = { mic: 0, system: 0 };
}

export async function initMeetingState() {
  try {
    const st = await invoke('meeting_status');
    if (st && st.active && st.meeting) setActive(st.meeting, st.elapsedMs || 0);
  } catch (_) { /* backend sin Notetaker: la página lo avisará */ }
  notify('meeting');

  listen('meeting://state', (e) => {
    if (!e) return;
    if (e.state === 'recording') {
      if (!live.active || (e.meetingId && live.meeting && live.meeting.id !== e.meetingId) || !live.meeting) {
        invoke('meeting_status').then((st) => {
          if (st && st.active && st.meeting) { setActive(st.meeting, st.elapsedMs || 0); notify('meeting', e); }
        }).catch(() => {});
      }
    } else if (e.state === 'summarizing') {
      if (e.meetingId != null) live.summarizing.add(e.meetingId);
      clearActive();
    } else if (e.state === 'done' || e.state === 'error') {
      if (e.meetingId != null) live.summarizing.delete(e.meetingId);
      if (e.state === 'error') clearActive();
    } else if (e.state === 'idle') clearActive();
    notify('meeting', e);
  });
  listen('meeting://segment', (seg) => {
    if (!seg) return;
    live.segments.push(seg);
    notify('meeting-segment', seg);
  });
  listen('meeting://level', (lv) => {
    if (!lv) return;
    live.level = lv;
    notify('meeting-level', lv);
  });
  listen('meeting://summary', (d) => {
    if (d) live.summarizing.delete(d.id);
    notify('meeting-summary', d);
  });
}

/** Marca como activa una reunión recién arrancada (respuesta de start_meeting). */
export function markStarted(meeting) {
  live.manual = false;
  if (live.active && live.meeting && live.meeting.id === meeting.id) { notify('meeting'); return; }
  setActive(meeting, 0);
  notify('meeting');
}
export function markStopped(meeting) {
  if (meeting) live.summarizing.add(meeting.id);
  clearActive();
  notify('meeting');
}
