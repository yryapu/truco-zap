// C4: no WebSocket a carta é o próprio caractere Unicode (decisão D04).
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, entrarNaFila, gravarWebSocket, conferirCartas } from './ajuda.js';

test('todo campo de carta no WebSocket é um único code point do bloco Playing Cards', async ({ page }) => {
  const quadros = gravarWebSocket(page); // antes de conectar
  await entrarComoConvidado(page);
  await entrarNaFila(page, '1x1', 0);
  await expect(page.getByTestId('mesa')).toBeVisible({ timeout: 45_000 });

  // Joga algumas cartas para gerar quadros de ida e volta.
  for (let i = 0; i < 6; i++) {
    const cartas = page.locator('[data-testid^="carta-"]:not([disabled])');
    if ((await cartas.count()) > 0) await cartas.first().click().catch(() => {});
    await page.waitForTimeout(900);
  }

  const vistas = conferirCartas(quadros.recebidos);
  expect(vistas, 'nenhuma carta trafegou — o teste não provou nada').toBeGreaterThan(10);

  // O que o cliente manda também é o caractere, não um objeto.
  const jogadas = quadros.enviados
    .map((p) => {
      try { return JSON.parse(p); } catch { return null; }
    })
    .filter((j) => j && j.t === 'jogar');
  expect(jogadas.length).toBeGreaterThan(0);
  for (const j of jogadas) {
    expect(typeof j.carta).toBe('string');
    expect([...j.carta].length, `mandei "${j.carta}"`).toBe(1);
    const cp = j.carta.codePointAt(0);
    expect(cp).toBeGreaterThanOrEqual(0x1f0a1);
    expect(cp).toBeLessThanOrEqual(0x1f0de);
  }

  // Exemplo do enunciado: o Ás de paus é 🃑 (U+1F0D1) e nenhum Cavaleiro (U+1F0xC) aparece.
  const todos = quadros.recebidos.join('');
  for (const cav of ['\u{1F0AC}', '\u{1F0BC}', '\u{1F0CC}', '\u{1F0DC}']) {
    expect(todos.includes(cav), 'Cavaleiro não existe no truco').toBe(false);
  }
});

test('a tela desenha a carta como caractere Unicode e o servidor recusa carta inventada', async ({ page }) => {
  await entrarComoConvidado(page);
  await entrarNaFila(page, '1x1', 0);
  await expect(page.getByTestId('mesa')).toBeVisible({ timeout: 45_000 });

  const cartas = page.locator('[data-testid^="carta-"]');
  await expect(cartas).toHaveCount(3);
  for (let i = 0; i < 3; i++) {
    const txt = await cartas.nth(i).textContent();
    expect([...txt.trim()].length).toBe(1);
    const cp = txt.trim().codePointAt(0);
    expect(cp).toBeGreaterThanOrEqual(0x1f0a1);
    expect(cp).toBeLessThanOrEqual(0x1f0de);
  }

});

test('cliente adulterado: o servidor recusa carta fora do baralho', async ({ page }) => {
  await entrarComoConvidado(page);
  // WebSocket cru, com o cookie da sessão. Isso derruba a conexão da página (uma conexão por
  // jogador, de propósito), o que não importa: o que se prova aqui é a recusa do servidor.
  const respostas = await page.evaluate(() => {
    return new Promise((ok) => {
      const vistos = [];
      const ws = new WebSocket(`ws://${location.host}/ws`);
      ws.onmessage = (e) => {
        vistos.push(JSON.parse(e.data));
        if (vistos.length >= 4) ok(vistos);
      };
      ws.onopen = () => {
        // U+1F0AC é o Cavaleiro de espadas: existe no Unicode, não existe no truco.
        ws.send(JSON.stringify({ t: 'jogar', carta: '\u{1F0AC}' }));
        // 10 (removido do baralho sujo) e uma string de dois code points.
        ws.send(JSON.stringify({ t: 'jogar', carta: '\u{1F0AA}' }));
        ws.send(JSON.stringify({ t: 'jogar', carta: '\u{1F0A1}\u{1F0A2}' }));
      };
      setTimeout(() => ok(vistos), 8000);
    });
  });
  const erros = respostas.filter((r) => r.t === 'erro').map((r) => r.codigo);
  expect(erros.length, JSON.stringify(respostas)).toBeGreaterThanOrEqual(3);
  for (const c of erros) expect(c).toBe('carta_desconhecida');
});
