// Script de inspeção visual, não é teste. Tira prints das três telas para eu criticar.
import { chromium } from '@playwright/test';
const BASE = process.env.BASE_URL || 'http://app:8080';
const b = await chromium.launch();

async function tela(nome, w, h, passos) {
  const ctx = await b.newContext({ viewport: { width: w, height: h }, locale: 'pt-BR' });
  const p = await ctx.newPage();
  await p.goto(BASE);
  await passos(p, ctx);
  await p.waitForTimeout(1200);
  await p.screenshot({ path: `/e2e/prints/${nome}.png`, fullPage: true });
  await ctx.close();
}

await tela('1-entrada-desktop', 1280, 900, async () => {});
await tela('2-entrada-mobile', 390, 844, async () => {});
await tela('3-lobby', 1280, 1000, async (p) => {
  await p.getByTestId('btn-convidado').click();
  await p.waitForTimeout(900);
});
await tela('4-lobby-mobile', 390, 900, async (p) => {
  await p.getByTestId('btn-convidado').click();
  await p.waitForTimeout(900);
});

// Mesa: dois jogadores de verdade, joga uma rodada inteira para ver a rodada resolvida.
{
  const c1 = await b.newContext({ viewport: { width: 1280, height: 1000 }, locale: 'pt-BR' });
  const c2 = await b.newContext({ viewport: { width: 1280, height: 1000 }, locale: 'pt-BR' });
  const [a, z] = [await c1.newPage(), await c2.newPage()];
  for (const p of [a, z]) {
    await p.goto(BASE);
    await p.getByTestId('btn-convidado').click();
    await p.waitForTimeout(700);
    await p.getByTestId('campo-aposta').fill('77');
    await p.getByTestId('btn-fila').click();
  }
  await a.waitForSelector('[data-testid="mesa"]', { timeout: 30000 });
  await a.waitForTimeout(1200);
  await a.screenshot({ path: '/e2e/prints/5-mesa-inicio.png', fullPage: true });

  // joga a 1a rodada
  for (let i = 0; i < 2; i++) {
    for (const p of [a, z]) {
      const c = p.locator('[data-testid^="carta-"]:not([disabled])');
      if ((await c.count()) > 0) { await c.first().click(); break; }
    }
    await a.waitForTimeout(500);
  }
  await a.waitForTimeout(900);
  await a.screenshot({ path: '/e2e/prints/6-mesa-rodada-resolvida.png', fullPage: true });

  // pede truco para ver o grito e as acoes
  const pedinte = (await a.getByTestId('btn-pedir').isVisible()) ? a : z;
  const outro = pedinte === a ? z : a;
  if (await pedinte.getByTestId('btn-pedir').isVisible()) {
    await pedinte.getByTestId('btn-pedir').click();
    await outro.waitForTimeout(250);
    await outro.screenshot({ path: '/e2e/prints/7-mesa-pedido.png', fullPage: true });
  }
  // mobile da mesa
  const c3 = await b.newContext({ viewport: { width: 390, height: 900 }, locale: 'pt-BR' });
  const m = await c3.newPage();
  await m.goto(BASE);
  await m.getByTestId('btn-convidado').click();
  await m.waitForTimeout(700);
  await m.getByTestId('campo-aposta').fill('5');
  await m.getByTestId('btn-fila').click();
  await m.waitForSelector('[data-testid="mesa"]', { timeout: 40000 });
  await m.waitForTimeout(3000);
  const cc = m.locator('[data-testid^="carta-"]:not([disabled])');
  if ((await cc.count()) > 0) await cc.first().click();
  await m.waitForTimeout(2500);
  await m.screenshot({ path: '/e2e/prints/8-mesa-mobile.png', fullPage: true });
  await c3.close();
  await c1.close(); await c2.close();
}
await b.close();
console.log('prints ok');
