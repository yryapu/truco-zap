// Mede, em canvas, onde o glifo de carta desenha dentro da caixa de em, para eu encaixar o
// fundo creme exatamente no contorno em vez de chutar.
import { chromium } from '@playwright/test';
const b = await chromium.launch();
const p = await b.newPage();
await p.goto(process.env.BASE_URL || 'http://app:8080');
await p.waitForTimeout(1500); // fontes
const r = await p.evaluate(async () => {
  await document.fonts.load('200px "Noto Sans Symbols 2"', '\u{1F0A1}');
  await document.fonts.ready;
  const S = 400;
  const c = document.createElement('canvas');
  c.width = c.height = S;
  const x = c.getContext('2d');
  x.fillStyle = '#000'; x.fillRect(0, 0, S, S);
  x.fillStyle = '#fff';
  x.font = '200px "Noto Sans Symbols 2"';
  x.textBaseline = 'alphabetic';
  x.fillText('\u{1F0A1}', 100, 300);
  const m = x.measureText('\u{1F0A1}');
  const d = x.getImageData(0, 0, S, S).data;
  let x0 = S, y0 = S, x1 = 0, y1 = 0;
  for (let yy = 0; yy < S; yy++) for (let xx = 0; xx < S; xx++) {
    if (d[(yy * S + xx) * 4] > 40) {
      if (xx < x0) x0 = xx; if (xx > x1) x1 = xx;
      if (yy < y0) y0 = yy; if (yy > y1) y1 = yy;
    }
  }
  return {
    fontePx: 200, origemX: 100, baseline: 300,
    avanco: m.width,
    tinta: { x0, y0, x1, y1, w: x1 - x0 + 1, h: y1 - y0 + 1 },
    familiaCarregada: document.fonts.check('200px "Noto Sans Symbols 2"', '\u{1F0A1}'),
  };
});
const { fontePx, origemX, baseline, avanco, tinta } = r;
console.log('fonte carregada:', r.familiaCarregada);
console.log('avanço (em):', (avanco / fontePx).toFixed(4));
console.log('largura do desenho (em):', (tinta.w / fontePx).toFixed(4));
console.log('altura do desenho  (em):', (tinta.h / fontePx).toFixed(4));
console.log('esquerda a partir da origem (em):', ((tinta.x0 - origemX) / fontePx).toFixed(4));
console.log('topo acima da baseline (em):', ((baseline - tinta.y0) / fontePx).toFixed(4));
console.log('base abaixo da baseline (em):', ((tinta.y1 - baseline) / fontePx).toFixed(4));
await b.close();
