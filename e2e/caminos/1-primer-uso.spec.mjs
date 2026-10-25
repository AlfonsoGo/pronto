// PRD §6.1 · Primer uso: abrir el hub sin modelo de voz → descargarlo → «listo para dictar».
// Mock: ?model=missing (descarga simulada ≈ 12 s + 0,9 s de verificación + 1,2 s de carga del motor).
export const nombre = 'primer-uso';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  await paso('abrir hub sin modelo', async ({ timeout }) => {
    await page.goto(url('model=missing'));
    await visible(page.getByRole('heading', { name: 'Falta el modelo de voz' }), timeout);
    await visible(page.getByRole('button', { name: 'Descargar modelo' }), timeout);
  });
  await paso('pulsar «Descargar modelo»', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Descargar modelo' }).click();
    await visible(page.getByRole('progressbar', { name: 'Descarga del modelo de voz' }), timeout);
  });
  await paso('descarga completa (la tarjeta desaparece)', async ({ timeout }) => {
    await page.getByRole('heading', { name: 'Falta el modelo de voz' }).waitFor({ state: 'detached', timeout });
  }, { timeout: 40000 });
  await paso('«Motor listo» (listo para dictar)', async ({ timeout }) => {
    await visible(page.getByText('Motor listo').first(), timeout);
  });
}
