// PRD §6.7 · Aviso de actualización disponible → «Actualizar».
// Mock: ?update=available (aviso ya visible); la instalación simulada dura ≈ 7 s (descarga 6 s + verificación 0,9 s).
export const nombre = 'actualizacion';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  await paso('aviso «Pronto 2.0.2 disponible»', async ({ timeout }) => {
    await page.goto(url('update=available'));
    await visible(page.getByRole('heading', { name: 'Pronto 2.0.2 disponible' }), timeout);
    await visible(page.getByRole('button', { name: 'Actualizar ahora' }), timeout);
  });
  await paso('pulsar «Actualizar ahora» → descarga', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Actualizar ahora' }).click();
    await visible(page.getByRole('progressbar', { name: 'Descarga de la actualización' }), timeout);
  });
  await paso('descarga → verificación → «Instalando…»', async ({ timeout }) => {
    await visible(page.getByText('Instalando… Pronto se cierra un momento y se abre solo.').first(), timeout);
  }, { timeout: 20000 });
}
