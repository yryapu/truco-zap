// C5: um integrador registra uma URL e recebe os eventos de partida, assinados.
import { test, expect } from '@playwright/test';
import http from 'node:http';
import crypto from 'node:crypto';
import os from 'node:os';
import { entrarComoConvidado, entrarNaFila, jogarAteOFim } from './ajuda.js';

const PORTA = Number(process.env.RECEPTOR_PORTA || 9099);

/// O endereço pelo qual o **servidor** alcança este processo de teste.
///
/// Usar o nome do serviço (`e2e`) não funciona: um container criado por `docker compose run`
/// é anexado à rede, mas o nome dele é `<projeto>-e2e-run-<hash>`, e o alias de serviço não é
/// garantido para containers avulsos. Foi exatamente isso que fez a primeira execução falhar
/// com "error sending request for url (http://e2e:9099/hook)" — ver ERROS.md E5.
/// IP próprio na rede do Compose não depende de DNS nenhum.
function meuEndereco() {
  if (process.env.RECEPTOR_HOST) return process.env.RECEPTOR_HOST;
  for (const ifaces of Object.values(os.networkInterfaces())) {
    for (const i of ifaces || []) {
      if (i.family === 'IPv4' && !i.internal) return i.address;
    }
  }
  return 'localhost';
}
const HOST = meuEndereco();

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

  await entrarNaFila(page, '1x1', 37);
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

// Independente do teste acima de propósito: um teste que só falha porque o anterior falhou
// não informa nada. Aqui a propriedade testada é a do HMAC em si.
test('a assinatura depende do segredo e do corpo', () => {
  const corpo = '{"evento":"partida.resultado","partida_id":"x"}';
  const h = (seg, c) => crypto.createHmac('sha256', seg).update(c).digest('hex');
  expect(h('segredo-a', corpo)).not.toBe(h('segredo-b', corpo));
  expect(h('segredo-a', corpo)).not.toBe(h('segredo-a', corpo + ' '));
  expect(h('segredo-a', corpo)).toBe(h('segredo-a', corpo));
  expect(h('segredo-a', corpo)).toHaveLength(64);
});
