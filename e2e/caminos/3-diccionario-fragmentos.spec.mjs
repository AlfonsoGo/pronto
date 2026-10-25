// PRD §6.3 · Añadir una palabra a «Mis palabras» y un atajo de texto, y verlos en su lista (nombres del 24-oct-2026).
export const nombre = 'diccionario-fragmentos';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  const palabra = 'Recorredorix', frase = 'mi telefono de prueba', expansion = '600 000 000';
  const dialogo = page.getByRole('dialog');
  await paso('abrir hub', async ({ timeout }) => {
    await page.goto(url());
    await visible(page.getByRole('heading', { level: 1 }), timeout);
  });
  await paso('ir a Mis palabras', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Mis palabras' }).click();
    await visible(page.getByRole('heading', { name: 'Mis palabras', level: 1 }), timeout);
    await visible(page.getByRole('button', { name: 'Agregar palabra', exact: true }), timeout);
  });
  await paso('abrir «Agregar palabra»', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Agregar palabra', exact: true }).click();
    await visible(dialogo.getByRole('heading', { name: 'Agregar palabra' }), timeout);
  });
  await paso('guardar la palabra', async ({ timeout }) => {
    await dialogo.getByLabel('¿Cómo se escribe?').fill(palabra);
    await dialogo.getByRole('button', { name: 'Guardar palabra' }).click();
    await dialogo.waitFor({ state: 'detached', timeout });
    await visible(page.getByText(`"${palabra}" se agregó correctamente`), timeout);
  });
  await paso('la palabra aparece en la lista', async ({ timeout }) => {
    await visible(page.locator('.vrow', { hasText: palabra }), timeout);
  });
  await paso('ir a Atajos de texto', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Atajos de texto' }).click();
    await visible(page.getByRole('heading', { name: 'Atajos de texto', level: 1 }), timeout);
  });
  await paso('abrir «Agregar atajo de texto»', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Agregar atajo', exact: true }).click();
    await visible(dialogo.getByRole('heading', { name: 'Agregar atajo de texto' }), timeout);
  });
  await paso('guardar el atajo', async ({ timeout }) => {
    await dialogo.getByLabel('Lo que dices', { exact: true }).fill(frase);
    await dialogo.getByLabel('Lo que se escribe').fill(expansion);
    await dialogo.getByRole('button', { name: 'Guardar atajo' }).click();
    await dialogo.waitFor({ state: 'detached', timeout });
    await visible(page.getByText(`"${frase}" se agregó correctamente`), timeout);
  });
  await paso('el atajo aparece en la lista', async ({ timeout }) => {
    const fila = page.locator('.vrow', { hasText: frase });
    await visible(fila, timeout);
    if (!(await fila.innerText()).includes(expansion)) throw new Error('la fila no muestra la expansión');
  });
}
