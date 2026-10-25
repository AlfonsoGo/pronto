// PRD §6.6 · Abrir una reunión terminada → ver resumen, tareas y quién dijo qué → preguntar sobre ella.
// Mock: reunión 501 «Revisión semanal de producto» (hecha); ask_meetings tarda 1,5 s simulados.
export const nombre = 'reunion';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  await paso('abrir Reuniones (lista de reuniones)', async ({ timeout }) => {
    await page.goto(url());
    await page.getByRole('button', { name: 'Reuniones' }).click();
    await visible(page.getByRole('link', { name: /Revisión semanal de producto/ }), timeout);
  });
  await paso('abrir la reunión terminada', async ({ timeout }) => {
    await page.getByRole('link', { name: /Revisión semanal de producto/ }).click();
    await visible(page.getByRole('heading', { name: 'Resumen' }), timeout);
  });
  await paso('ver resumen, tareas y quién dijo qué', async ({ timeout }) => {
    await visible(page.locator('.m-sum'), timeout);
    await visible(page.getByRole('heading', { name: 'Tareas pendientes' }), timeout);
    await visible(page.getByRole('heading', { name: 'Quién dijo qué' }), timeout);
    if ((await page.locator('.todo li').count()) < 1) throw new Error('no hay tareas en la lista');
    if ((await page.locator('.mt-said li').count()) < 1) throw new Error('«Quién dijo qué» está vacío');
  });
  await paso('preguntar sobre la reunión', async ({ timeout }) => {
    const campo = page.getByRole('textbox', { name: 'Pregunta sobre esta reunión' });
    await campo.fill('¿Qué se decidió?');
    await campo.press('Enter');
    await visible(page.locator('.ask-a'), timeout);
    const txt = (await page.locator('.ask-a').innerText()).trim();
    if (!txt) throw new Error('la respuesta de Ask llegó vacía');
  });
}
