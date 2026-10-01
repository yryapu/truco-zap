// C6: ranking e emblemas de reputação.
import { test, expect } from '@playwright/test';
import { entrarComoConvidado, abrirMesa, jogarAteOFim } from './ajuda.js';

test('ganhar uma partida sobe o jogador no ranking e troca o emblema de trilha', async ({ browser }) => {
  const ctxs = [await browser.newContext(), await browser.newContext()];
  const pages = [await ctxs[0].newPage(), await ctxs[1].newPage()];
  for (const p of pages) await entrarComoConvidado(p);
  const apelidos = await Promise.all(pages.map((p) => p.getByTestId('perfil-apelido').textContent()));

  for (const p of pages) {
    await expect(p.getByTestId('emblemas').locator('[data-chave="estreante"]')).toBeVisible();
  }

  await abrirMesa(pages, '1x1', 0);
  const vencedor = await jogarAteOFim(pages);
  await vencedor.getByTestId('btn-voltar').click();

  // Trilha por vitórias: 0 => estreante, 1 => pé-de-meia.
  await expect(vencedor.getByTestId('emblemas').locator('[data-chave="pe_de_meia"]')).toBeVisible();
  await expect(vencedor.getByTestId('emblemas').locator('[data-chave="estreante"]')).toHaveCount(0);

  // E aparece no ranking com 1 vitória.
  const rk = vencedor.getByTestId('ranking');
  await expect(rk).toContainText('1V / 0D');
  for (const a of apelidos) await expect(rk).toContainText(a);

  // Histórico do vencedor mostra a partida terminada.
  await expect(vencedor.getByTestId('historico')).toContainText('1x1');
  for (const c of ctxs) await c.close();
});
