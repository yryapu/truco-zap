// C1: cadastro rápido — entra e começa a jogar em menos de um minuto.
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, cadastrar, entrarNaFila } from './ajuda.js';

test('um clique vira jogador com 1000 moedas e emblema de estreante', async ({ page }) => {
  const t0 = Date.now();
  const apelido = await entrarComoConvidado(page);
  expect(apelido).toMatch(/^Visitante \d{4}$/);
  await expect(page.getByTestId('emblemas').locator('[data-chave="estreante"]')).toBeVisible();
  expect(Date.now() - t0, 'cadastro tem de caber em muito menos de 60s').toBeLessThan(20_000);
});

test('do primeiro clique até a mesa aberta em menos de 60 segundos, jogando sozinho', async ({ page }) => {
  const t0 = Date.now();
  await entrarComoConvidado(page);
  await entrarNaFila(page, '1x1', 7);
  // Casa vazia: a mesa é completada com robôs depois de 8s (decisão D15).
  await expect(page.getByTestId('mesa')).toBeVisible({ timeout: 45_000 });
  await expect(page.getByTestId('minha-mao').locator('[data-testid^="carta-"]')).toHaveCount(3);
  const decorrido = Date.now() - t0;
  expect(decorrido, `levou ${decorrido}ms`).toBeLessThan(60_000);
});

test('conta com apelido e senha: cadastra, sai e volta', async ({ page }) => {
  const apelido = `Zeca${Date.now() % 100000}`;
  await cadastrar(page, apelido);
  await expect(page.getByTestId('perfil-apelido')).toHaveText(apelido);
  await page.getByTestId('btn-sair').click();
  await expect(page.getByTestId('btn-convidado')).toBeVisible();
  await page.getByText('Já tenho conta').click();
  await page.getByTestId('campo-apelido').fill(apelido);
  await page.getByTestId('campo-senha').fill('segredo123');
  await page.getByTestId('btn-entrar').click();
  await expect(page.getByTestId('perfil-apelido')).toHaveText(apelido);
});

test('apelido repetido e senha errada são recusados com mensagem', async ({ page }) => {
  const apelido = `Dino${Date.now() % 100000}`;
  await cadastrar(page, apelido);
  await page.getByTestId('btn-sair').click();
  await page.getByText('Já tenho conta').click();
  await page.getByTestId('campo-apelido').fill(apelido);
  await page.getByTestId('campo-senha').fill('segredo123');
  await page.getByTestId('btn-cadastrar').click();
  await expect(page.getByTestId('erro-entrada')).toHaveText(/apelido já existe/);
  await page.getByTestId('campo-senha').fill('senhaerrada');
  await page.getByTestId('btn-entrar').click();
  await expect(page.getByTestId('erro-entrada')).toHaveText(/não conferem/);
});
