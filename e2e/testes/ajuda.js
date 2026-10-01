// Helpers compartilhados. Nenhuma regra de truco aqui — os testes de regra vivem em Rust
// (`cargo test -p truco_core`). Aqui provamos a tela e o WebSocket.
import { expect } from '@playwright/test';

export const BLOCO_CARTAS = { min: 0x1f0a0, max: 0x1f0de };

export async function entrarComoConvidado(page) {
  await page.goto('/');
  await page.getByTestId('btn-convidado').click();
  await expect(page.getByTestId('perfil-saldo')).toHaveText('1000');
  return (await page.getByTestId('perfil-apelido').textContent()) ?? '';
}

export async function cadastrar(page, apelido, senha = 'segredo123') {
  await page.goto('/');
  await page.getByText('tenho / quero uma conta').click();
  await page.getByTestId('campo-apelido').fill(apelido);
  await page.getByTestId('campo-senha').fill(senha);
  await page.getByTestId('btn-cadastrar').click();
}

export async function entrarNaFila(page, modo, aposta) {
  await page.getByTestId(`modo-${modo}`).click();
  await page.getByTestId('campo-aposta').selectOption(String(aposta));
  await page.getByTestId('btn-fila').click();
}

/// Põe N jogadores na mesma fila e espera a mesa abrir para todos.
export async function abrirMesa(pages, modo, aposta) {
  for (const p of pages) await entrarNaFila(p, modo, aposta);
  for (const p of pages) {
    await expect(p.getByTestId('mesa')).toBeVisible({ timeout: 30_000 });
  }
}

async function visivel(loc) {
  return loc.isVisible().catch(() => false);
}

/// Dirige a(s) página(s) até alguém ganhar. Política: aceita tudo, nunca pede truco (os
/// pedidos têm teste próprio), joga a primeira carta disponível.
export async function jogarAteOFim(pages, limiteMs = 100_000) {
  const inicio = Date.now();
  while (Date.now() - inicio < limiteMs) {
    for (const p of pages) {
      if (await visivel(p.getByTestId('fim'))) return p;
      for (const id of ['btn-onze-jogar', 'btn-aceitar']) {
        const b = p.getByTestId(id);
        if (await visivel(b)) await b.click({ timeout: 2000 }).catch(() => {});
      }
      const cartas = p.locator('[data-testid^="carta-"]:not([disabled])');
      if ((await cartas.count().catch(() => 0)) > 0) {
        await cartas.first().click({ timeout: 2000 }).catch(() => {});
      }
    }
    await pages[0].waitForTimeout(120);
  }
  throw new Error('nenhuma partida terminou dentro do limite');
}

/// Grava todos os quadros WebSocket de uma página. Precisa ser chamado antes de conectar.
export function gravarWebSocket(page) {
  const quadros = { enviados: [], recebidos: [] };
  page.on('websocket', (ws) => {
    ws.on('framesent', (f) => quadros.enviados.push(f.payload));
    ws.on('framereceived', (f) => quadros.recebidos.push(f.payload));
  });
  return quadros;
}

/// Toda carta que aparece num quadro tem de ser UM code point do bloco Playing Cards.
export function conferirCartas(payloads) {
  let vistas = 0;
  const recolher = (v) => {
    if (typeof v === 'string') return;
    if (Array.isArray(v)) return v.forEach(recolher);
    if (v && typeof v === 'object') {
      for (const [k, val] of Object.entries(v)) {
        const ehCampoDeCarta = k === 'carta' || k === 'vira' || k === 'manilha_rank';
        if (ehCampoDeCarta && typeof val === 'string') {
          const pontos = [...val];
          expect(pontos.length, `"${val}" (campo ${k}) devia ser 1 code point`).toBe(1);
          const cp = val.codePointAt(0);
          expect(cp, `${k}=${val} U+${cp.toString(16)} fora do bloco Playing Cards`)
            .toBeGreaterThanOrEqual(BLOCO_CARTAS.min);
          expect(cp).toBeLessThanOrEqual(BLOCO_CARTAS.max);
          vistas++;
        } else if (k === 'minha_mao' && Array.isArray(val)) {
          for (const c of val) {
            expect([...c].length).toBe(1);
            vistas++;
          }
        } else {
          recolher(val);
        }
      }
    }
  };
  for (const p of payloads) {
    let j;
    try {
      j = JSON.parse(p);
    } catch {
      continue;
    }
    recolher(j);
  }
  return vistas;
}
