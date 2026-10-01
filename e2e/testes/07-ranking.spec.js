// C6: ranking e emblemas de reputação.
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, abrirMesa, jogarAteOFim, quemVenceu } from './ajuda.js';

test('ganhar uma partida sobe o jogador no ranking e troca o emblema de trilha', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);
  const apelidos = await Promise.all(pages.map((p) => p.getByTestId('perfil-apelido').textContent()));

  for (const p of pages) {
    await expect(p.getByTestId('emblemas').locator('[data-chave="estreante"]')).toBeVisible();
  }

  await abrirMesa(pages, '1x1', 41);
  await jogarAteOFim(pages);
  // `jogarAteOFim` devolve quem VIU o fim primeiro, e o fim aparece para os dois.
  const vencedor = await quemVenceu(pages);
  await vencedor.getByTestId('btn-voltar').click();

  // Trilha por vitórias: 0 => estreante, 1 => pé-de-meia.
  await expect(vencedor.getByTestId('emblemas').locator('[data-chave="pe_de_meia"]')).toBeVisible();
  await expect(vencedor.getByTestId('emblemas').locator('[data-chave="estreante"]')).toHaveCount(0);

  // E se acha na classificação, com 1 vitória — mesmo que o topo já esteja cheio de outros.
  // Procurar o apelido no top da lista era o teste errado: com o banco cheio o jogador novo
  // simplesmente não está lá, e isso é um problema do produto, não do teste (ERROS.md E16).
  await expect(vencedor.getByTestId('ranking')).toContainText('1V / 0D');
  const minha = vencedor.getByTestId('minha-posicao');
  await expect(minha).toContainText('(você)');
  await expect(minha).toContainText('1V / 0D');
  const meuApelido = await vencedor.getByTestId('perfil-apelido').textContent();
  await expect(minha).toContainText(meuApelido);
  expect(apelidos).toContain(meuApelido);

  // Histórico do vencedor mostra a partida terminada.
  await expect(vencedor.getByTestId('historico')).toContainText('1x1');
  for (const c of ctxs) await c.close();
});

test('a sua linha aparece na classificação mesmo com o topo cheio de outros', async ({ page }) => {
  await entrarComoConvidado(page);
  const apelido = await page.getByTestId('perfil-apelido').textContent();
  const minha = page.getByTestId('minha-posicao');
  await expect(minha).toContainText(apelido);
  await expect(minha).toContainText('(você)');
  await expect(minha).toContainText('0V / 0D');
  // A posição é um número de verdade, não um placeholder.
  const pos = await minha.locator('.pos').textContent();
  expect(Number(pos), `posição lida: ${pos}`).toBeGreaterThan(0);
});
