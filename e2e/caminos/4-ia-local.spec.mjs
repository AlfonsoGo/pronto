// PRD §6.4 · Activar la IA local en un botón y encender «Pulir cada dictado con IA local».
// Mock: ?ollama=none (Ollama sin instalar). La activación simulada dura ≈ 28 s (descarga 9 s, instalación 6,5 s, arranque 2,5 s, modelo 8,7 s).
export const nombre = 'ia-local';

export async function correr({ page, url, paso }) {
  const visible = (loc, timeout) => loc.waitFor({ state: 'visible', timeout });
  const dialogo = page.getByRole('dialog');
  // Desde el 24-oct-2026 el pulido vive en Configuración → Limpieza e IA (antes, Estilo → Limpieza automática).
  await paso('abrir Configuración → Limpieza e IA', async ({ timeout }) => {
    await page.goto(url('ollama=none'));
    await visible(page.getByRole('heading', { level: 1 }), timeout);
    await page.getByRole('button', { name: 'Configuración' }).click();
    await dialogo.getByRole('tab', { name: 'Limpieza e IA' }).click();
    await visible(page.getByRole('button', { name: 'Activar IA local' }), timeout);
  });
  await paso('pulsar «Activar IA local» → aviso previo', async ({ timeout }) => {
    await page.getByRole('button', { name: 'Activar IA local' }).click();
    await visible(dialogo.getByRole('heading', { name: 'Activar la IA local' }), timeout);
  });
  await paso('«Instalar» → empieza el progreso', async ({ timeout }) => {
    await dialogo.getByRole('button', { name: 'Instalar' }).click();
    await visible(page.getByRole('progressbar', { name: 'Activando la IA local' }), timeout);
  });
  await paso('«IA local lista»', async ({ timeout }) => {
    await visible(page.getByText('IA local lista').first(), timeout);
  }, { timeout: 60000 });
  await paso('encender «Pulir cada dictado con IA local»', async ({ timeout }) => {
    const sw = page.getByRole('switch', { name: 'Pulir cada dictado con IA local' });
    await visible(sw, timeout);
    await sw.click();
    await page.locator('[role=switch][aria-checked=true][aria-label="Pulir cada dictado con IA local"]').waitFor({ state: 'visible', timeout });
    await visible(page.getByText('Pulido con IA activado'), timeout);
  });
}
