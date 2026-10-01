// Playwright escolhido porque o requisito a provar é "duas sessões de navegador
// independentes jogam uma partida inteira por WebSocket" — contextos isolados, cookies
// separados e espera por evento, nativamente. Justificativa completa: decisão D13.
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './testes',
  timeout: 180_000,
  expect: { timeout: 20_000 },
  // Sequencial: as suítes compartilham um servidor e um banco; paralelismo aqui compraria
  // segundos e pagaria em testes intermitentes.
  workers: 1,
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: process.env.BASE_URL || 'http://localhost:8080',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    locale: 'pt-BR',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
