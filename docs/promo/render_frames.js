// 240fps 4× 子帧渲染 → ffmpeg tmix=4 降采到 60fps（按 Opus 5.5 视频技法）
// 用法: node render_frames.js            —— 单进程渲染全部 5760 帧
//       node render_frames.js 0 2        —— 双进程并行: worker 0 / 共 2 个
const puppeteer = require('/tmp/opencode/node_modules/puppeteer-core');
const fs = require('fs');
const path = require('path');
const PAGE = '/tmp/opencode/promo/typesake_promo.html';
const OUT = '/tmp/opencode/promo/fsub';
const TOTAL = 5760;                    // 24s × 240fps
const W = +(process.argv[2] || 0);
const N = +(process.argv[3] || 1);
const PER = Math.ceil(TOTAL / N);
const lo = W * PER, hi = Math.min(lo + PER, TOTAL);
(async () => {
  fs.mkdirSync(OUT, { recursive: true });
  const browser = await puppeteer.launch({
    executablePath: '/usr/bin/chromium-headless-shell',
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--font-render-hinting=none'],
    protocolTimeout: 180000,
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1920, height: 1080, deviceScaleFactor: 1 });
  const errs = [];
  page.on('pageerror', e => errs.push(String(e)));
  await page.goto('file://' + PAGE, { waitUntil: 'networkidle0' });
  const t0 = Date.now();
  for (let i = lo; i < hi; i++) {
    await page.evaluate(tt => window.seek(tt), i / 240);
    await page.screenshot({ path: path.join(OUT, `f_${String(i).padStart(5, '0')}.png`) });
    if ((i - lo) % 240 === 0) console.log(`w${W} ${i - lo}/${hi - lo} ${((Date.now() - t0) / 1000) | 0}s`);
  }
  await browser.close();
  console.log('done', lo, hi, 'page errors:', errs.length ? errs : 'none');
})().catch(e => { console.error(e); process.exit(1); });
