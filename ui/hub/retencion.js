// Cuándo acortar la conservación del historial o del audio necesita confirmación (lo usa Configuración → Privacidad y
// datos; lo prueba tests/retencion.test.mjs sin navegador). Acortar borra al momento y para siempre (reglas 2 y 5).

/** ¿El valor nuevo conserva menos que el guardado? Historial: 0 = siempre (lo más largo). Audio: 0 = no guardar (lo más corto). */
export function shortensRetention(key, prev, next) {
  if (prev === next) return false;
  if (key === 'historyRetentionDays') return next !== 0 && (prev === 0 || next < prev);
  if (key === 'keepAudioDays') return next === 0 || (prev !== 0 && next < prev);
  return false;
}
