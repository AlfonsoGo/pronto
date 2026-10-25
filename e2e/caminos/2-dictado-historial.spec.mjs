// PRD §6.2 · Un dictado llega → aparece arriba en el historial → copiarlo o reintentar su transcripción.
// Mock: dictation://result cada 20 s desde que carga el hub (el recorrido espera al evento real, sin emitirlo a mano).
export const nombre = 'dictado-historial';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  const filas = page.locator('article.hrow');
  let antes = { n: 0, primera: '' };
  await paso('abrir hub con historial', async ({ timeout }) => {
    await page.goto(url());
    await visible(filas.first(), timeout);
    antes = { n: await filas.count(), primera: (await filas.first().locator('.h-text').innerText()).trim() };
  });
  await paso('llega un dictado nuevo (evento del mock, cada 20 s)', async ({ timeout }) => {
    await page.waitForFunction(({ n, primera }) => {
      const f = document.querySelectorAll('article.hrow');
      return f.length > n && f[0].querySelector('.h-text').textContent.trim() !== primera;
    }, antes, { timeout, polling: 100 });
  }, { timeout: 30000 });
  await paso('aparece arriba, marcado como nuevo', async ({ timeout }) => {
    await visible(filas.first(), timeout);
    const txt = (await filas.first().locator('.h-text').innerText()).trim();
    if (!txt || txt === antes.primera) throw new Error('la fila de arriba no es el dictado nuevo');
    await visible(page.getByText(/Nuevo dictado: \d+ palabras/), timeout);
  });
  await paso('copiar transcripción', async ({ timeout }) => {
    await filas.first().hover(); // las acciones de la fila solo reciben clics con el ratón encima (hub.css .h-acts)
    await filas.first().getByRole('button', { name: 'Copiar transcripción' }).click();
    await visible(page.getByText('Transcripción copiada'), timeout);
  });
  await paso('reintentar transcripción', async ({ timeout }) => {
    await filas.first().hover();
    await filas.first().getByRole('button', { name: 'Más opciones' }).click();
    await page.getByRole('menuitem', { name: 'Reintentar transcripción' }).click();
    await visible(page.getByText('Transcripción actualizada'), timeout);
  });
}
