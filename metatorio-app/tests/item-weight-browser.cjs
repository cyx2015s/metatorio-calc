const { withBrowser } = require('./browser-harness.cjs');
const { seed, waitFor } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({ port: 5207 }, async (browser, url) => {
    const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
    const errors = []; page.on('pageerror', error => errors.push(String(error)));
    await seed(page, 5);
    await page.addInitScript(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await invoke(command, args);
        if (command === 'prototype_detail' && args.kind === 'item') {
          result.calculation.stats = [
            { key: 'item-weight', value: 500, unit: 'grams' },
            { key: 'stack-weight', value: 50000, unit: 'grams' },
            { key: 'rocket-item-capacity', value: 2000, unit: 'number' },
          ];
          result.calculation.item_weight = { weight: 500, source: 'recipe', recipe: 'iron-plate' };
        }
        return result;
      };
    });
    await page.goto(url);
    await page.locator('.row-item').filter({ has: page.locator('.flow-name') }).first().locator('.hover-icon').hover();
    const tooltip = page.getByRole('tooltip'); await tooltip.waitFor();
    for (const text of ['由配方推导', '单件重量', '500 g', '每组重量', '50 kg', '标准火箭载量（件）', '2000', '重量不随品质变化']) {
      assert.ok((await tooltip.textContent()).includes(text), text);
    }
    assert.equal(await tooltip.locator('.flow-row .rich-text').count(), 1);
    assert.equal(await tooltip.evaluate(el => el.scrollWidth > el.clientWidth), false);
    if (process.env.WEIGHT_SCREENSHOT) await page.screenshot({ path: process.env.WEIGHT_SCREENSHOT });
    assert.deepEqual(errors, []);
    console.log('Item weight hover DOM passed: shared facts, mass units, recipe attribution, quality note and no overflow.');
}).catch(error => { console.error(error); process.exitCode = 1; });
