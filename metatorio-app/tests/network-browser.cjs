const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const {seed, waitFor} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs/promises');
const {spawn} = require('node:child_process');
const appRoot = path.resolve(__dirname, '..'), port = Number(process.env.UI_TEST_PORT || 5203);
const url = `http://127.0.0.1:${port}`;
(async () => {
  const server = spawn(process.execPath, [path.join(appRoot, 'node_modules/vite/bin/vite.js'),
    ...(process.env.NETWORK_PREVIEW ? ['preview'] : []), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], {cwd: appRoot, windowsHide: true, stdio: 'ignore'});
  let browser;
  try {
    await waitFor(async () => { try { return (await fetch(url)).ok; } catch { return false; } }, 600);
    browser = await chromium.launch({headless: true, ...(process.env.BROWSER_EXECUTABLE ? {executablePath: process.env.BROWSER_EXECUTABLE} : {})});
    const page = await browser.newPage({viewport: {width: 1600, height: 1100}, deviceScaleFactor: 2, acceptDownloads: true});
    page.setDefaultTimeout(30000);
    const errors = [], requests = [];
    page.on('pageerror', error => errors.push(String(error)));
    page.on('request', request => requests.push(request.url()));
    await seed(page, 96);
    await page.addInitScript(() => {
      localStorage.setItem('metatorio-ui-font', 'Courier New');
      localStorage.setItem('metatorio-mono-font', 'Consolas');
      const raw = window.__TAURI_INTERNALS__.invoke;
      let failed = false;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const value = await raw(command, args);
        if (command !== 'mechanic_flow') return value;
        if (args.mechanic === 95 && !failed) { failed = true; throw Error('retryable expansion failure'); }
        const item = (id, quality = 'normal') => ({Item: {id, quality}});
        if (args.mechanic === 1) return [[item('iron-plate'), 2], [item('iron-ore'), -2], ['Electricity', -24000]];
        if (args.mechanic === 2) return [[item('iron-plate', 'rare'), 0.2], [item('iron-ore'), -2]];
        if (args.mechanic === 3) return [[item('iron-ore'), 4]];
        if (args.mechanic === 4) return [[{Fluid: {name: 'steam', temperature: [165, 165]}}, 3]];
        if (args.mechanic === 7) return [[{Fluid: {name: 'steam', temperature: [100, 500]}}, -3], ['Electricity', 90000]];
        if (args.mechanic === 9) return [[{VirtualScience: 'iron-plate'}, 1]];
        return [[item('coal'), 1], ['Electricity', -1000]];
      };
    });
    await page.goto(url);
    await page.locator('.mech-card').first().waitFor();
    assert.equal(await page.getByRole('button', {name: '列表', exact: true}).getAttribute('aria-pressed'), 'true');
    assert.ok(!requests.some(request => /cytoscape|elk\.bundled/.test(request)), 'graph engines remain lazy in the default list');
    await page.getByRole('button', {name: '网络', exact: true}).click();
    const graph = page.getByRole('region', {name: '机制网络视图', exact: true});
    await graph.getByText('1 个机制展开失败', {exact: true}).waitFor();
    await graph.getByRole('button', {name: '重试展开', exact: true}).click();
    const ready = async () => {
      try {
        await waitFor(async () => await graph.getAttribute('data-network-ready') === 'true' &&
          !/展开 \d+ \/|展开失败|布局中/.test(await graph.textContent()), 600);
      } catch (error) {
        throw Error(`${error}\n${JSON.stringify({errors, text: await page.locator('.center').textContent()})}`);
      }
    };
    await ready();
    assert.ok(await graph.locator('canvas').count() > 0, 'Cytoscape rendered a real canvas');
    assert.equal(await graph.locator('.network-credit a').getAttribute('href'), 'https://github.com/YoisakiKanade222');
    assert.equal(await graph.getByRole('link', {name: '来源与许可', exact: true}).count(), 0);
    // Inspect the real renderer behind the canvas, rather than only CSS or
    // requested options. _cyreg is test-only access to the pinned Cytoscape API.
    const renderer = async () => graph.locator('.network-canvas').evaluate(node => {
      const cy = node._cyreg.cy;
      const canvas = [...node.querySelectorAll('canvas')].find(canvas => canvas.clientWidth > 0);
      return {zoom: cy.zoom(), ratio: canvas.width / canvas.clientWidth,
        nodeFont: cy.nodes().first().style('font-family'),
        rateFont: cy.edges('[kind="flow"]').first().style('font-family')};
    });
    assert.ok((await renderer()).ratio >= 1.9, 'canvas backing pixels follow the high-DPI screen');
    assert.ok((await renderer()).nodeFont.includes('Courier New'), 'labels use the saved global UI font');
    assert.ok((await renderer()).rateFont.includes('Consolas'), 'quantities use the saved global code font');
    await graph.locator('.network-canvas').hover({position: {x: 150, y: 150}});
    const zoomBefore = (await renderer()).zoom;
    for (let i = 0; i < 12; i++) {
      await page.mouse.wheel(0, -100);
      await page.waitForTimeout(20);
    }
    await waitFor(async () => (await renderer()).zoom > zoomBefore * 1.25);
    const viewportNodeScans = await graph.locator('.network-canvas').evaluate(node => {
      const cy = node._cyreg.cy, original = cy.nodes;
      let scans = 0;
      cy.nodes = function (...args) { scans++; return original.apply(this, args); };
      try { cy.panBy({x: 3, y: 0}); } finally { cy.nodes = original; }
      return scans;
    });
    assert.equal(viewportNodeScans, 0, 'panning must not scan or copy every node position');
    await page.getByRole('button', {name: '应用菜单', exact: true}).click();
    await page.getByRole('button', {name: '字体设置…', exact: true}).click();
    const fontSettings = page.getByRole('dialog', {name: '字体设置', exact: true});
    await fontSettings.getByLabel('界面字体', {exact: true}).fill('Arial');
    await fontSettings.getByLabel('代码字体', {exact: true}).fill('Courier New');
    await page.keyboard.press('Escape');
    await waitFor(async () => (await renderer()).nodeFont.includes('Arial') && (await renderer()).rateFont.includes('Courier New'));
    assert.equal(await page.evaluate(() => new Set(window.__layoutCalls.filter(c => c.command === 'mechanic_flow').map(c => c.args.mechanic)).size), 96);
    await graph.getByRole('checkbox', {name: '能量与污染', exact: true}).check();
    await ready();
    await graph.getByRole('textbox', {name: '搜索网络节点'}).fill('Electricity');
    await graph.locator('.node-directory .node-options button').filter({hasText: '电力'}).first().click();
    await graph.getByRole('region', {name: '网络对象详情'}).waitFor();
    assert.equal(await graph.getByRole('region', {name: '网络对象详情'}).locator('[data-flow-kind="Electricity"]').count(), 1);
    await graph.getByRole('textbox', {name: '搜索网络节点'}).fill('铁板');
    await graph.locator('.node-directory .node-options button').filter({hasText: '个机制'}).first().click();
    const card = graph.locator('.network-details .mech-card');
    await card.waitFor();
    await card.getByRole('switch', {name: '停用机制', exact: true}).click();
    await card.getByRole('switch', {name: '启用机制', exact: true}).waitFor();
    await ready();
    assert.ok((await page.evaluate(() => window.__layoutCalls.filter(c => c.command === 'dispatch')))
      .some(call => call.args.message.action.factory === 2 && call.args.message.action.action['mechanic-list']?.['set-enabled']?.mechanic === 1));
    await card.getByRole('switch', {name: '启用机制', exact: true}).click();
    await ready();
    await page.getByRole('button', {name: '列表', exact: true}).click();
    assert.equal(await graph.count(), 0);
    await page.locator('.center').evaluate(node => { node.scrollTop = 1200; });
    await page.waitForTimeout(100);
    await page.getByRole('button', {name: '网络', exact: true}).click();
    await ready();
    assert.equal(await graph.getByRole('textbox', {name: '搜索网络节点'}).inputValue(), '铁板');
    assert.ok(await graph.getByRole('checkbox', {name: '能量与污染', exact: true}).isChecked());
    assert.equal(await graph.locator('.network-details .mech-card').count(), 1, 'selection survives switching views');
    await page.getByRole('button', {name: '列表', exact: true}).click();
    await waitFor(async () => Math.abs(await page.locator('.center').evaluate(node => node.scrollTop) - 1200) < 2);
    await page.getByRole('button', {name: '网络', exact: true}).click();
    await ready();
    await graph.getByRole('button', {name: '重排', exact: true}).click();
    await ready();
    const downloadPromise = page.waitForEvent('download');
    await graph.getByRole('button', {name: '导出 PNG', exact: true}).click();
    const download = await downloadPromise;
    const png = await fs.readFile(await download.path());
    assert.equal(png.subarray(1, 4).toString(), 'PNG');
    assert.ok(png.readUInt32BE(16) > 100 && png.readUInt32BE(20) > 100);
    await graph.locator('.network-canvas').hover({position: {x: 150, y: 150}});
    await page.mouse.wheel(0, -500);
    await page.waitForTimeout(200);
    const secondDownloadPromise = page.waitForEvent('download');
    await graph.getByRole('button', {name: '导出 PNG', exact: true}).click();
    const secondPng = await fs.readFile(await (await secondDownloadPromise).path());
    assert.deepEqual([secondPng.readUInt32BE(16), secondPng.readUInt32BE(20)], [png.readUInt32BE(16), png.readUInt32BE(20)], 'whole-network export ignores viewport zoom');
    if (process.env.NETWORK_SCREENSHOT) {
      await graph.getByRole('button', {name: '定位匹配', exact: true}).click();
      await page.waitForTimeout(200);
      await page.screenshot({path: process.env.NETWORK_SCREENSHOT});
    }
    await page.getByRole('button', {name: '筛选', exact: true}).click();
    const filter = page.getByRole('region', {name: '机制流筛选'});
    await filter.getByRole('button', {name: '选择产出对象', exact: true}).click();
    await filter.getByRole('textbox', {name: '搜索筛选对象'}).fill('铁板');
    await filter.locator('.flow-option').filter({has: page.locator('[data-flow-kind="Item"]')}).first().click();
    await filter.getByLabel('产出品质').selectOption('rare');
    await page.getByRole('button', {name: '列表', exact: true}).click();
    await waitFor(async () => await page.locator('[data-mechanic-row]').count() === 1);
    assert.equal(await page.locator('[data-mechanic-row]').getAttribute('data-mechanic-row'), '2');
    await page.getByRole('button', {name: '网络', exact: true}).click();
    await ready();
    assert.equal(await filter.getByLabel('产出品质').inputValue(), 'rare');
    assert.ok((await graph.textContent()).includes('1 个机制节点'));
    await page.setViewportSize({width: 800, height: 700});
    assert.equal(await page.evaluate(() => document.body.scrollWidth > innerWidth), false);
    assert.ok(requests.every(request => request.startsWith(url) || request.startsWith('blob:') || request.startsWith('data:')), 'offline dependencies: no CDN requests');
    assert.deepEqual(errors, []);
    console.log('Network browser passed: lazy engines, full cached expansion/retry, canvas, shared editing/filter, state restoration, PNG, narrow viewport and offline assets.');
  } finally { await browser?.close(); server.kill(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
