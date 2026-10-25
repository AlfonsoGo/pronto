// PRD §6.5 · Cambiar de motor: descargar Whisper → el botón pasa a «Usar» sin salir de la pestaña → usarlo.
// El mock imita `modeldl::whisper_start`: progreso y un `done` con el nombre del fichero (regresión del 18-oct).
export const nombre = 'cambiar-motor';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  const dialogo = page.getByRole('dialog');
  const fila = () => dialogo.locator('.srow', { hasText: 'Whisper small' });
  await paso('abrir Configuración → Motor', async ({ timeout }) => {
    await page.goto(url());
    await visible(page.getByRole('heading', { level: 1 }), timeout);
    await page.getByRole('button', { name: 'Configuración' }).click();
    await dialogo.getByRole('tab', { name: 'Reconocimiento de voz' }).click();
    await visible(fila().getByRole('button', { name: 'Descargar' }), timeout);
  });
  await paso('pulsar «Descargar» en Whisper small', async ({ timeout }) => {
    await fila().getByRole('button', { name: 'Descargar' }).click();
    await visible(fila().getByText(/Descargando… \d+ %/), timeout);
  });
  await paso('el botón pasa a «Usar» sin salir de la pestaña', async ({ timeout }) => {
    try { await visible(fila().getByRole('button', { name: 'Usar' }), timeout); }
    catch { throw new Error('tras 8 s la fila no pasa a «Usar»: el hub no repinta al terminar la descarga (fallo del 18-oct).'); }
  }, { timeout: 8000 });
  await paso('pulsar «Usar»', async ({ timeout }) => {
    await fila().getByRole('button', { name: 'Usar' }).click();
    await visible(fila().getByText(/Cargando…|En uso|Elegido/).first(), timeout);
  });
}
