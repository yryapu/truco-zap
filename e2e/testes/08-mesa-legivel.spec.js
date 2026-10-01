// A mesa tem de ser ACOMPANHÁVEL: dá para ver a carta do adversário ao lado da sua e quem
// levou a rodada. Este arquivo existe porque a primeira versão falhava nisso — a mesa esvaziava
// no instante em que a rodada resolvia.
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, abrirMesa } from './ajuda.js';

test('a rodada que acabou fica na mesa, com as duas cartas e a vencedora marcada', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);
  await abrirMesa(pages, '1x1', 43);

  // Joga a primeira rodada inteira: um de cada lado.
  for (let i = 0; i < 2; i++) {
    const quem = pages.find(async () => true);
    for (const p of pages) {
      const c = p.locator('[data-testid^="carta-"]:not([disabled])');
      if ((await c.count()) > 0) {
        await c.first().click();
        break;
      }
    }
    await pages[0].waitForTimeout(400);
    void quem;
  }

  for (const p of pages) {
    const anterior = p.locator('.bloco-anterior');
    await expect(anterior, 'a rodada resolvida continua visível').toBeVisible();
    // As duas cartas da rodada, não só a minha.
    await expect(anterior.locator('.na-mesa')).toHaveCount(2);
    // E dá para ver de quem é cada uma.
    const nomes = await anterior.locator('.de-quem').allTextContents();
    expect(nomes.filter((n) => n.trim().length > 0)).toHaveLength(2);
    // Empate é possível; se houve vencedor, exatamente uma carta está marcada como "levou".
    const texto = await anterior.locator('.titulo-anterior').textContent();
    if (!texto.includes('empatou')) {
      await expect(anterior.locator('.na-mesa.levou')).toHaveCount(1);
      await expect(anterior.getByText('levou')).toBeVisible();
    }
    // O marcador das três rodadas mostra a 1ª resolvida e as outras duas em aberto.
    await expect(p.getByTestId('rodadas').locator('.marca')).toHaveCount(3);
    await expect(p.getByTestId('rodadas').locator('.marca.nossa, .marca.deles, .marca.empate'))
      .toHaveCount(1);
  }
  for (const c of ctxs) await c.close();
});

test('o placar é 12 sementes por lado e enche conforme os pontos', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);
  await abrirMesa(pages, '1x1', 47);

  for (const p of pages) {
    await expect(p.locator('#sementes-nos i')).toHaveCount(12);
    await expect(p.locator('#sementes-eles i')).toHaveCount(12);
    await expect(p.locator('#sementes-nos i.cheia')).toHaveCount(0);
  }

  // Um pede truco, o outro corre: 1 ponto, 1 semente.
  const pedinte = await Promise.race(
    pages.map((p) => p.getByTestId('btn-pedir').waitFor({ state: 'visible' }).then(() => p))
  );
  const outro = pedinte === pages[0] ? pages[1] : pages[0];
  await pedinte.getByTestId('btn-pedir').click();
  await outro.getByTestId('btn-correr').click();

  await expect(pedinte.locator('#sementes-nos i.cheia')).toHaveCount(1);
  await expect(outro.locator('#sementes-eles i.cheia')).toHaveCount(1);
  await expect(outro.locator('#sementes-nos i.cheia')).toHaveCount(0);
  for (const c of ctxs) await c.close();
});

test('as telas não se sobrepõem: hidden ganha de qualquer display declarado', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#tela-entrada')).toBeVisible();
  await expect(page.locator('#tela-lobby')).toBeHidden();
  await expect(page.locator('#tela-mesa')).toBeHidden();
  await entrarComoConvidado(page);
  await expect(page.locator('#tela-entrada')).toBeHidden();
  await expect(page.locator('#tela-lobby')).toBeVisible();
  await expect(page.locator('#caixa-fim')).toBeHidden();
});
