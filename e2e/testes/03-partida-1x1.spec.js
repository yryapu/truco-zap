// C3/C7: partida 1x1 inteira entre dois navegadores independentes, por WebSocket.
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, abrirMesa, jogarAteOFim } from './ajuda.js';

test('dois jogadores jogam uma partida 1x1 do início ao fim e o saldo muda', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);

  await abrirMesa(pages, '1x1', 100);

  // A aposta é debitada na abertura: 1000 - 100.
  for (const p of pages) {
    await expect(p.getByTestId('bolo')).toHaveText('200');
    await expect(p.getByTestId('jogadores').locator('.j')).toHaveCount(2);
    await expect(p.getByTestId('minha-mao').locator('[data-testid^="carta-"]')).toHaveCount(3);
  }
  // Exatamente um dos dois tem a vez na primeira rodada.
  const vezes = await Promise.all(pages.map((p) => p.getByTestId('vez').textContent()));
  expect(vezes.filter((v) => v.includes('sua vez')).length).toBe(1);

  await jogarAteOFim(pages);

  for (const p of pages) await expect(p.getByTestId('fim')).toBeVisible();
  const textos = await Promise.all(pages.map((p) => p.getByTestId('fim').textContent()));
  expect(textos.filter((t) => t.includes('Vitória')).length, textos.join(' | ')).toBe(1);
  expect(textos.filter((t) => t.includes('Derrota')).length).toBe(1);

  // Soma zero: o vencedor leva o bolo (200), o perdedor fica com 900.
  const saldos = await Promise.all(pages.map((p) => p.getByTestId('perfil-saldo').textContent()));
  const nums = saldos.map(Number).sort((a, b) => a - b);
  expect(nums, saldos.join(',')).toEqual([900, 1100]);

  for (const c of ctxs) await c.close();
});

test('truco sobe o valor da mão de 1 para 3 e o adversário vê a resposta', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);
  await abrirMesa(pages, '1x1', 0);

  for (const p of pages) await expect(p.getByTestId('valor-mao')).toHaveText('1');

  // Quem tem a vez pede truco.
  const pedinte = (await pages[0].getByTestId('btn-pedir').isVisible()) ? pages[0] : pages[1];
  const respondente = pedinte === pages[0] ? pages[1] : pages[0];
  await expect(pedinte.getByTestId('btn-pedir')).toContainText('TRUCO');
  await pedinte.getByTestId('btn-pedir').click();

  await expect(respondente.getByTestId('btn-aceitar')).toBeVisible();
  await expect(respondente.getByTestId('btn-aumentar')).toBeVisible();
  await expect(respondente.getByTestId('btn-correr')).toBeVisible();
  await respondente.getByTestId('btn-aumentar').click(); // aceita 3 e pede 6

  for (const p of pages) await expect(p.getByTestId('valor-mao')).toHaveText('3');
  await expect(pedinte.getByTestId('btn-aceitar')).toBeVisible();
  await pedinte.getByTestId('btn-aceitar').click();
  for (const p of pages) await expect(p.getByTestId('valor-mao')).toHaveText('6');

  // Quem acabou de pedir não pode pedir de novo (R9).
  await expect(pedinte.getByTestId('btn-pedir')).toBeHidden();
  await expect(respondente.getByTestId('btn-pedir')).toBeHidden();

  for (const c of ctxs) await c.close();
});

test('correr do truco entrega 1 ponto, não 3', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);
  await abrirMesa(pages, '1x1', 0);

  const pedinte = (await pages[0].getByTestId('btn-pedir').isVisible()) ? pages[0] : pages[1];
  const respondente = pedinte === pages[0] ? pages[1] : pages[0];
  await pedinte.getByTestId('btn-pedir').click();
  await respondente.getByTestId('btn-correr').click();

  await expect(pedinte.getByTestId('placar-nos')).toHaveText('1');
  await expect(respondente.getByTestId('placar-eles')).toHaveText('1');
  await expect(pedinte.getByTestId('log')).toContainText('1 ponto(s)');
  for (const c of ctxs) await c.close();
});
