// C2: sessão segura, dados de cada jogador isolados dos outros.
import { test, expect, request } from '@playwright/test';
import { entrarComoConvidado } from './ajuda.js';

const base = process.env.BASE_URL || 'http://localhost:8080';

test('sem cookie, nada pessoal é servido', async () => {
  const anon = await request.newContext({ baseURL: base });
  for (const rota of ['/api/eu', '/api/historico', '/api/webhooks']) {
    expect((await anon.get(rota)).status(), rota).toBe(401);
  }
  // Rota pública continua pública.
  expect((await anon.get('/api/ranking')).status()).toBe(200);
  await anon.dispose();
});

test('cookie inventado não vira sessão', async () => {
  const falso = await request.newContext({
    baseURL: base,
    extraHTTPHeaders: { cookie: 'truco_sessao=' + 'A'.repeat(43) },
  });
  expect((await falso.get('/api/eu')).status()).toBe(401);
  await falso.dispose();
});

test('o webhook de um jogador não aparece para o outro', async ({ browser }) => {
  const ctxA = await browser.newContext();
  const ctxB = await browser.newContext();
  const [a, b] = [await ctxA.newPage(), await ctxB.newPage()];
  const apA = await entrarComoConvidado(a);
  const apB = await entrarComoConvidado(b);
  expect(apA).not.toBe(apB);

  await a.getByTestId('webhook-url').fill('https://example.com/so-do-a');
  await a.getByTestId('btn-webhook').click();
  await expect(a.getByTestId('webhooks').locator('li')).toHaveCount(1);
  await expect(a.getByTestId('webhook-segredo')).toContainText('segredo');

  await b.reload();
  await expect(b.getByTestId('webhooks').locator('li')).toHaveCount(0);
  await expect(b.getByTestId('webhooks')).not.toContainText('so-do-a');
  await ctxA.close();
  await ctxB.close();
});

test('o cookie de sessão é HttpOnly — JavaScript da página não o alcança', async ({ page, context }) => {
  await entrarComoConvidado(page);
  const visivelAoJs = await page.evaluate(() => document.cookie);
  expect(visivelAoJs).not.toContain('truco_sessao');
  const cookies = await context.cookies();
  const c = cookies.find((x) => x.name === 'truco_sessao');
  expect(c, 'o cookie existe no navegador').toBeTruthy();
  expect(c.httpOnly).toBe(true);
  expect(c.sameSite).toBe('Lax');
});
