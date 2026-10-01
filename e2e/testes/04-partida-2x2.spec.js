// C3: partida 2x2 (duplas) com quatro navegadores independentes.
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, abrirMesa, jogarAteOFim } from './ajuda.js';

// Uma partida 2x2 até 12 pontos são muitas mãos vezes quatro navegadores. O limite global de
// 120s não cabe aqui, e aumentar o limite é mais honesto que encurtar a partida.
test.setTimeout(420_000);

test('quatro jogadores formam duas duplas e a partida termina', async ({ browser }) => {
  const ctxs = [];
  const pages = [];
  for (let i = 0; i < 4; i++) {
    const c = await browser.newContext();
    ctxs.push(c);
    const p = await c.newPage();
    pages.push(p);
    await entrarComoConvidado(p);
  }

  await abrirMesa(pages, '2x2', 23);

  for (const p of pages) {
    await expect(p.getByTestId('jogadores').locator('.j')).toHaveCount(4);
    await expect(p.getByTestId('bolo')).toHaveText('92');
    // Cada um vê 2 do próprio lado (incluindo si) e 2 do outro.
    await expect(p.getByTestId('jogadores')).toContainText('nós');
    await expect(p.getByTestId('jogadores')).toContainText('eles');
    const nos = await p.getByTestId('jogadores').locator('.j', { hasText: 'nós' }).count();
    expect(nos).toBe(2);
  }
  // Nenhum robô: os quatro assentos são humanos.
  for (const p of pages) {
    await expect(p.getByTestId('jogadores')).not.toContainText('🤖');
  }

  await jogarAteOFim(pages, 360_000);

  const textos = await Promise.all(pages.map((p) => p.getByTestId('fim').textContent()));
  expect(textos.filter((t) => t.includes('Vitória')).length, textos.join(' | ')).toBe(2);
  expect(textos.filter((t) => t.includes('Derrota')).length).toBe(2);

  // Aposta 23 de cada: bolo 92, dividido entre os 2 vencedores => 46 cada.
  // Perdedor: 1000-23 = 977. Vencedor: 977+46 = 1023. Soma preservada: 4000.
  const saldos = (await Promise.all(pages.map((p) => p.getByTestId('perfil-saldo').textContent())))
    .map(Number)
    .sort((a, b) => a - b);
  expect(saldos).toEqual([977, 977, 1023, 1023]);
  expect(saldos.reduce((a, b) => a + b, 0), 'moeda nao e criada nem destruida').toBe(4000);

  for (const c of ctxs) await c.close();
});
