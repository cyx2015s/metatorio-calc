// Exercise the shared runtime and real Svelte views with mocked catalog IPC.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { seed, waitFor } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawn } = require('node:child_process');
const appRoot = path.resolve(__dirname, '..');
const port = Number(process.env.UI_TEST_PORT || 5199);
const url = `http://127.0.0.1:${port}`;

(async () => {
  const server = spawn(process.execPath, [path.join(appRoot, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], {cwd: appRoot, windowsHide: true, stdio: 'ignore'});
  let browser;
  try {
    await waitFor(async () => { try { return (await fetch(url)).ok; } catch { return false; } }, 600);
    browser = await chromium.launch({headless: true, ...(process.env.BROWSER_EXECUTABLE ? {executablePath: process.env.BROWSER_EXECUTABLE} : {})});
    const page = await browser.newPage({viewport: {width: 1600, height: 1000}});
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    await seed(page, 16);
    await page.addInitScript(() => {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await invoke(command, args);
        if (command === 'catalog_index') Object.assign(result.names, {
          'item-subgroup/test': '[color=red]测试小组[/color]',
          'fuel-category/chemical': '化学燃料',
          'module-category/speed': '速度类别',
          'recipe-category/crafting': '制造类别',
          'surface-property/custom': '自定义表面属性',
          'space-location/edge': '星系边缘',
        });
        if (command === 'prototype_detail') {
          result.categories = ['crafting'];
          result.fuel_categories = ['chemical'];
          result.allowed_module_categories = ['speed'];
          result.calculation.surface_properties = {custom: 42};
        }
        return result;
      };
    });
    await page.goto(url);
    const target = page.locator('.row-item').filter({has: page.locator('.flow-name')}).first();
    await target.waitFor();
    await target.locator('.hover-icon').hover();
    const tooltip = page.getByRole('tooltip');
    await tooltip.waitFor();
    await waitFor(async () => (await tooltip.textContent()).includes('化学燃料'));
    for (const name of ['制造类别', '速度类别', '自定义表面属性']) assert.ok((await tooltip.textContent()).includes(name), name);
    await page.mouse.move(1200, 80);
    await tooltip.waitFor({state: 'detached'});
    await target.locator('.hover-icon').click();
    await page.locator('.sg-label').first().waitFor();
    assert.equal(await page.locator('.sg-label').first().textContent(), '测试小组');
    assert.equal(await page.locator('.sg-label [style*="color"]').first().textContent(), '测试小组');
    await page.keyboard.press('Escape');
    await page.getByRole('tab').filter({hasText: '项目'}).click();
    assert.equal(await page.locator('#quality-limit option[value="rare"]').textContent(), '稀有');
    assert.equal(await page.locator('#quality-limit option[value="rare"]').getAttribute('value'), 'rare');
    const labels = await page.evaluate(async () => {
      const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
      const old = runtime.localizedName('quality', 'rare');
      const index = JSON.parse(JSON.stringify(runtime.catalogIndex));
      index.entries.find(entry => entry.kind === 'quality' && entry.name === 'rare').localized_name = '刷新后的稀有';
      runtime.catalogIndex = index;
      return {
        old, updated: runtime.localizedName('quality', 'rare'),
        location: runtime.localizedName('space-location', 'edge'),
        wrongCategory: runtime.localizedName('fuel-category', 'iron-plate'),
        icon: runtime.localizedName('flow', 'Electricity'),
      };
    });
    assert.deepEqual(labels, {old: '稀有', updated: '刷新后的稀有', location: '星系边缘', wrongCategory: 'iron-plate', icon: '电力'});
    await waitFor(async () => (await page.locator('#quality-limit option[value="rare"]').textContent()) === '刷新后的稀有');
    await page.evaluate(async () => {
      const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
      runtime.solve = {project: 1, factory: 2, status: {'not-solved': {
        description: '测试无解',
        no_provider: [{Item: {id: 'iron-plate', quality: 'rare'}}],
        no_consumer: [{Fluid: {name: 'steam', temperature: [165, 500]}}],
      }}};
    });
    await page.locator('.err-box').filter({hasText: '测试无解'}).waitFor();
    const failure = await page.locator('.err-box').filter({hasText: '测试无解'}).textContent();
    assert.ok(failure.includes('铁板（刷新后的稀有）'));
    assert.ok(failure.includes('蒸汽（165～500℃）'));
    assert.ok(!failure.includes('iron-plate'));
    assert.deepEqual(errors, []);
    console.log('Localized labels DOM passed: categories, rich subgroup, quality values, namespace isolation, same-context refresh, space locations, synthetic flows and infeasibility diagnostics.');
  } finally {
    await browser?.close();
    server.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
