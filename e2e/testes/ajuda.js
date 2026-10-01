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

/// Entra na fila. **Cada teste usa uma aposta única** de propósito: a fila é indexada por
/// (modo, aposta), então apostas distintas dão a cada teste uma fila isolada. Sem isso, dois
/// testes com a mesma aposta se emparelham entre si e um deles fica esperando para sempre —
/// foi o que me deu falhas intermitentes na primeira execução (ERROS.md E7).
export async function entrarNaFila(page, modo, aposta) {
  await page.getByTestId(`modo-${modo}`).click();
  await page.getByTestId('campo-aposta').fill(String(aposta));
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

/// Um passo de jogo, resolvido dentro da página num único round-trip.
///
/// Usa `.click()` do DOM em vez de `locator.click()` de propósito: o papel desta função é
/// *avançar o jogo* até o fim, e uma partida inteira são dezenas de jogadas — quatro
/// `isVisible()` por página por iteração custava mais que o jogo. Os cliques que precisam ser
/// cliques de verdade (cadastro, pedir truco, aceitar, correr, escolher carta) têm testes
/// próprios que usam `locator.click()` com as checagens de actionability do Playwright.
async function passo(page) {
  return page.evaluate(() => {
    const el = (sel) => document.querySelector(sel);
    const mostrado = (e) => e && !e.hidden && e.offsetParent !== null && !e.disabled;
    if (mostrado(el('[data-testid="fim"]'))) return 'fim';
    for (const id of ['btn-onze-jogar', 'btn-aceitar']) {
      const b = el(`[data-testid="${id}"]`);
      if (mostrado(b)) {
        b.click();
        return id;
      }
    }
    const c = el('[data-testid^="carta-"]:not([disabled])');
    if (c) {
      c.click();
      return 'carta';
    }
    return 'nada';
  });
}

/// Dirige a(s) página(s) até alguém ganhar. Política: aceita tudo, nunca pede truco (os
/// pedidos têm teste próprio), joga a primeira carta disponível. Devolve a página que venceu.
export async function jogarAteOFim(pages, limiteMs = 180_000) {
  const inicio = Date.now();
  let paradas = 0;
  while (Date.now() - inicio < limiteMs) {
    let mexeu = false;
    for (const p of pages) {
      const r = await passo(p).catch(() => 'nada');
      if (r === 'fim') return p;
      if (r !== 'nada') mexeu = true;
    }
    paradas = mexeu ? 0 : paradas + 1;
    // Nada aconteceu: ou é a pausa entre mãos, ou o servidor está pensando. Espera pouco.
    await pages[0].waitForTimeout(mexeu ? 40 : 200);
    if (paradas > 400) throw new Error('o jogo parou de avançar — ninguém tem ação possível');
  }
  const fins = await Promise.all(
    pages.map((p) => p.getByTestId('fim').textContent().catch(() => '—'))
  );
  throw new Error(`nenhuma partida terminou em ${limiteMs}ms. Estado de fim: ${fins.join(' | ')}`);
}

/// Qual das páginas venceu. `jogarAteOFim` devolve a primeira que *viu* o fim, e o fim
/// aparece para os dois — vencedor e perdedor. Confundir as duas coisas foi E6.
export async function quemVenceu(pages) {
  for (const p of pages) {
    const t = await p.getByTestId('fim').textContent().catch(() => '');
    if ((t || '').includes('Vitória')) return p;
  }
  throw new Error('ninguém mostra Vitória — a partida não terminou como eu esperava');
}

/// Devolve a página cujo jogador tem a vez e pode pedir truco, esperando até aparecer.
export async function quemPodePedir(pages) {
  return Promise.race(
    pages.map((p) =>
      p.getByTestId('btn-pedir').waitFor({ state: 'visible', timeout: 25_000 }).then(() => p)
    )
  );
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
