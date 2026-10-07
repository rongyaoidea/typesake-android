const puppeteer = require('/tmp/opencode/node_modules/puppeteer-core');
const fs = require('fs');
const PAGE = '/tmp/opencode/promo/typesake_promo.html';
const path=require('path');
const OUT = '/tmp/opencode/promo/frames';
(async () => {
  fs.rmSync(OUT, { recursive: true, force: true });
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await puppeteer.launch({
    executablePath: '/usr/bin/chromium-headless-shell',
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--font-render-hinting=none'],
    protocolTimeout: 120000,
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
  await page.goto('file://' + PAGE, { waitUntil: 'networkidle0' });
  const T0 = 0, T1 = 24.0, FPS = 15;
  const total = Math.floor((T1 - T0) * FPS);
  for (let i = 0; i < total; i++) {
    const t = T0 + i / FPS;
    await page.evaluate((tt) => window.seek(tt), t);
    const file = path.join(OUT, `f_${String(i).padStart(4, '0')}.png`);
    await page.screenshot({ path: file, omitBackground: false });
    if (i % 50 === 0) console.log('frame', i, 't=', t.toFixed(2));
  }
  await browser.close();
  console.log('done, frames:', total);
})().catch(e => { console.error(e); process.exit(1); });
