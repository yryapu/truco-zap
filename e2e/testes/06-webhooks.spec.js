// C5: um integrador registra uma URL e recebe os eventos de partida, assinados.
import { test, expect } from '@playwright/test';
import http from 'node:http';
import crypto from 'node:crypto';
import { entrarComoConvidado, entrarNaFila, jogarAteOFim } from './ajuda.js';

const PORTA = Number(process.env.RECEPTOR_PORTA || 9099);
const HOST = process.env.RECEPTOR_HOST || 'localhost';

let servidor;
let recebidos = [];

test.beforeAll(async () => {
  servidor = http.createServer((req, res) => {
    let corpo = '';
    req.on('data', (c) => (corpo += c));
    req.on('end', () => {
      recebidos.push({ headers: req.headers, corpo });
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end('{"ok":true}');
    });
  });
  await new Promise((ok) => servidor.listen(PORTA, '0.0.0.0', ok));
});

test.afterAll(async () => {
  await new Promise((ok) => servidor.close(ok));
});

test('registra webhook e recebe partida.comecou e partida.resultado com HMAC válido', async ({ page }) => {
  recebidos = [];
  await entrarComoConvidado(page);

  await page.getByTestId('webhook-url').fill(`http://${HOST}:${PORTA}/hook`);
  await page.getByTestId('btn-webhook').click();
  const linha = await page.getByTestId('webhook-segredo').textContent();
  const segredo = linha.split(': ').pop().trim();
  expect(segredo.length).toBeGreaterThan(20);

  await entrarNaFila(page, '1x1', 0);
  await expect(page.getByTestId('mesa')).toBeVisible({ timeout: 45_000 });
  await jogarAteOFim([page]);

  // A entrega é assíncrona (fila no banco + trabalhador em background).
  await expect.poll(() => recebidos.map((r) => r.headers['x-truco-evento']), { timeout: 40_000 })
    .toEqual(expect.arrayContaining(['partida.comecou', 'partida.terminou', 'partida.resultado']));

  for (const r of recebidos) {
    const esperada = 'sha256=' + crypto.createHmac('sha256', segredo).update(r.corpo).digest('hex');
    expect(r.headers['x-truco-signature'], 'assinatura do corpo').toBe(esperada);
    const j = JSON.parse(r.corpo);
    expect(j.evento_id, 'todo evento traz evento_id para o receptor ser idempotente').toBeTruthy();
    expect(j.partida_id).toBeTruthy();
  }

  const resultado = JSON.parse(
    recebidos.find((r) => r.headers['x-truco-evento'] === 'partida.resultado').corpo
  );
  expect(resultado.placar.some((p) => p >= 12), JSON.stringify(resultado.placar)).toBe(true);
  expect(resultado.time_vencedor).toBeGreaterThanOrEqual(0);
  expect(resultado.jogadores.length).toBeGreaterThan(0);
});

test('a assinatura não fecha com o segredo errado', async () => {
  expect(recebidos.length).toBeGreaterThan(0);
  const r = recebidos[0];
  const errada = 'sha256=' + crypto.createHmac('sha256', 'nao-e-o-segredo').update(r.corpo).digest('hex');
  expect(r.headers['x-truco-signature']).not.toBe(errada);
});
