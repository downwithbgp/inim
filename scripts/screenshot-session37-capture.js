#!/usr/bin/env node
// Session 37 — workbench screenshot capture driver.
//
// Reads capture descriptors from argv: NAME|URL|WIDTH|HEIGHT|MARKER
// (one per line on stdin), then for each:
//   - creates a browser context with the requested viewport (DPR 1)
//   - navigates, waits for load, verifies the document content marker
//   - captures a full-page PNG (width must equal the viewport width;
//     height may exceed the viewport for full-page captures)
//   - writes <outdir>/<name>-<W>x<H>.png
//
// Fails (exit 1) when the page errors or the marker is absent. The
// driver prints one line per capture: NAME|PATH|WIDTH|HEIGHT.
//
// The playwright module is resolved like the session-36 harness's
// `npx --no-install playwright` (node module search from the repo root).

const fs = require('fs');
const path = require('path');

let playwrightPath;
try {
  playwrightPath = require.resolve('playwright');
} catch (e) {
  console.error('playwright module not resolvable (run: npm i -g playwright)');
  process.exit(3);
}
const { chromium } = require(playwrightPath);

const OUT_DIR = process.argv[2];
if (!OUT_DIR) {
  console.error('usage: screenshot-session37-capture.js <outdir>');
  process.exit(3);
}
fs.mkdirSync(OUT_DIR, { recursive: true });

const lines = fs.readFileSync(0, 'utf8').trim().split('\n').filter(Boolean);

(async () => {
  const browser = await chromium.launch();
  let failed = false;
  for (const line of lines) {
    const parts = line.split('|');
    const [name, url, w, h, marker] = parts;
    const fullPage = parts.length > 5 ? parts[5] === 'full' : true;
    const width = parseInt(w, 10);
    const height = parseInt(h, 10);
    const outFile = path.join(OUT_DIR, `${name}-${width}x${height}.png`);
    const context = await browser.newContext({
      viewport: { width, height },
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();
    let status = 0;
    try {
      const resp = await page.goto(url, { waitUntil: 'load' });
      status = resp ? resp.status() : 0;
      if (status !== 200) {
        console.error(`FAILED: ${name} — HTTP ${status} (page error)`);
        failed = true;
        await context.close();
        continue;
      }
      const content = await page.content();
      if (!content.includes(marker)) {
        console.error(`FAILED: ${name} — expected marker absent: ${marker}`);
        failed = true;
        await context.close();
        continue;
      }
      await page.screenshot({ path: outFile, fullPage });
      console.log(`${name}|${outFile}|${width}|${height}`);
    } catch (err) {
      console.error(`FAILED: ${name} — ${err.message}`);
      failed = true;
    } finally {
      await context.close();
    }
  }
  await browser.close();
  process.exit(failed ? 1 : 0);
})();
