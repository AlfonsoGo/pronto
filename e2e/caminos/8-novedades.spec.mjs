// Camino 8 (2.3.0, al día con la 2.3.1) · Tras actualizar: «Lo nuevo» sale una vez → «Probar» de «Enséñale cómo dices una palabra» abre
// «Agregar palabra» con «Grabar pronunciación» resaltado → «Novedades» del menú abre la página con la versión 2.3.1.
// Mock: hub.html?novedades fuerza la tarjeta (en la maqueta, sin ese parámetro no sale para no tapar las demás pantallas).
export const nombre = 'novedades';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  await paso('tras actualizar sale «Lo nuevo en Pronto 2.3.1»', async ({ timeout }) => {
    await page.goto(url('novedades'));
    await visible(page.getByRole('heading', { name: 'Lo nuevo en Pronto 2.3.1' }), timeout);
    await visible(page.getByRole('button', { name: 'Probar: Enséñale cómo dices una palabra' }), timeout);
  });
  await paso('«Probar» abre «Agregar palabra» con «Grabar pronunciación» resaltado', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Probar: Enséñale cómo dices una palabra' }).click();
    const dlg = page.getByRole('dialog').filter({ has: page.getByRole('heading', { name: 'Agregar palabra' }) });
    await visible(dlg, timeout);
    const rec = dlg.getByRole('button', { name: 'Grabar pronunciación' });
    await visible(rec, timeout);
    await page.waitForFunction(() => document.querySelector('.modal [data-pron=rec]')?.classList.contains('nv-spot'), null, { timeout });
    await page.waitForFunction(() => document.activeElement === document.querySelector('.modal [data-pron=rec]'), null, { timeout });
  });
  await paso('cerrar «Agregar palabra»; «Lo nuevo» no vuelve (sale una vez)', async ({ timeout }) => {
    await page.getByRole('dialog').getByRole('button', { name: 'Cancelar' }).click();
    await page.getByRole('dialog').waitFor({ state: 'detached', timeout });
    await page.waitForTimeout(600); // margen para detectar una reaparición
    if (await page.getByRole('heading', { name: 'Lo nuevo en Pronto 2.3.1' }).count()) throw new Error('la tarjeta «Lo nuevo» ha vuelto a salir');
  });
  await paso('«Novedades» del menú abre la página con la versión 2.3.1', async ({ timeout }) => {
    await page.getByRole('button', { name: /^Novedades/ }).click();
    await visible(page.getByRole('heading', { name: 'Novedades', level: 1 }), timeout);
    await visible(page.getByText('Versión 2.3.1', { exact: true }), timeout);
    await visible(page.getByRole('heading', { name: 'Arreglos de la 2.3' }), timeout);
    // La 2.3.0 queda en «Versiones anteriores», plegada con su título.
    await visible(page.getByText('Más fácil, más rápido y con reuniones de verdad', { exact: true }), timeout);
  });
}
