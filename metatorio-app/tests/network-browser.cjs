const {installNetworkPngMock}=require('./network-png-mock.cjs');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const {seed, waitFor} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
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
      window.__pngExports = [];
      let failed = false;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'icon') {
          const canvas = document.createElement('canvas');
          canvas.width = canvas.height = 32;
          const ctx = canvas.getContext('2d');
          ctx.fillStyle = args.ty === 'quality' ? '#237bea' : '#b8b8b8';
          ctx.fillRect(0, 0, 32, 32);
          return Array.from(atob(canvas.toDataURL().split(',')[1]), c => c.charCodeAt(0));
        }
        const value = await raw(command, args);
        if (command === 'dispatch') {
          const edit = args.message.action?.action?.mechanic;
          const entry = window.__layoutDocument.projects[0].factories[0].mechanics.find(entry => entry.id === edit?.mechanic);
          const action = edit?.action?.recipe;
          if (action?.['set-recipe']) entry.mechanic.recipe = action['set-recipe'].recipe;
          if (action?.['set-machine']) entry.mechanic.machine = action['set-machine'].machine;
        }
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
    await page.addInitScript(installNetworkPngMock,{deferDialog:true});
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
    if (process.env.NETWORK_TEST_TRACE) console.log('network ready');
    assert.ok(await graph.locator('canvas').count() > 0, 'Cytoscape rendered a real canvas');
    const nodeIcons = () => graph.locator('.network-canvas').evaluate(node => {
      const cy = node._cyreg.cy;
      const normal = cy.nodes().filter(n => n.data('kind') === 'flow' && n.data('label') === '铁板\n普通').first();
      const rare = cy.nodes().filter(n => n.data('kind') === 'flow' && n.data('label') === '铁板\n稀有').first();
      const merged = cy.nodes().filter(n => n.data('kind') === 'mechanic' && n.data('label').includes('个机制') && n.data('label').startsWith('铁板')).first();
      const layers = n => n.pstyle('background-image').value.length;
      return {normal: layers(normal), rare: layers(rare), merged: layers(merged), width: merged.width(), fontSize: merged.numericStyle('font-size'), edgeFontSize: cy.edges().filter(e=>e.data('kind')!=='compatibility').first().numericStyle('font-size'),
        zoom: cy.zoom(), pan: cy.pan(), positions: cy.nodes().map(n => [n.id(), n.position()])};
    });
    await waitFor(async () => (await nodeIcons()).rare === 2 && (await nodeIcons()).merged === 3);
    const originalIcons = await nodeIcons();
    assert.equal(originalIcons.normal, 1, 'ordinary quality needs no corner badge');
    await graph.getByLabel('网络显示大小', {exact: true}).selectOption('2');
    await waitFor(async () => (await nodeIcons()).width === originalIcons.width * 2);
    const largeIcons = await nodeIcons();
    assert.equal(largeIcons.fontSize, originalIcons.fontSize * 2);
    assert.equal(largeIcons.edgeFontSize, originalIcons.edgeFontSize * 2);
    assert.equal(largeIcons.zoom, originalIcons.zoom, 'icon size is independent of viewport zoom');
    assert.deepEqual(largeIcons.pan, originalIcons.pan);
    assert.deepEqual(largeIcons.positions, originalIcons.positions, 'changing icon size does not rearrange nodes');
    assert.equal(await page.locator('.network-legend').count(), 0, 'legend and attribution are collapsed by default');
    const canvasBeforeLegend = await graph.locator('.network-canvas').boundingBox();
    await graph.getByRole('button', {name: '图例与来源', exact: true}).click();
    const legend = page.getByRole('dialog', {name: '图例与来源', exact: true});
    await legend.waitFor();
    assert.equal(await legend.locator('.network-credit a').getAttribute('href'), 'https://github.com/YoisakiKanade222');
    assert.match(await legend.textContent(), /未显示求解器的降温与提热辅助变量/);
    const canvasWithLegend = await graph.locator('.network-canvas').boundingBox();
    assert.deepEqual([canvasWithLegend.width, canvasWithLegend.height], [canvasBeforeLegend.width, canvasBeforeLegend.height], 'opening help does not squeeze the canvas');
    await legend.getByRole('button', {name: '关闭', exact: true}).click();
    assert.equal(await graph.locator(':scope > .network-legend, :scope > .network-credit').count(), 0, 'there is no footer below the canvas');
    const stageBox = await graph.locator('.network-stage').boundingBox(), graphBox = await graph.boundingBox();
    assert.ok(Math.abs(graphBox.y + graphBox.height - (stageBox.y + stageBox.height)) < 3, 'canvas fills the bottom of the network workspace');
    assert.equal(await graph.getByRole('link', {name: '来源与许可', exact: true}).count(), 0);
    assert.equal(await graph.locator('.node-directory').count(), 0, 'search results do not take a separate row');
    const search = graph.getByRole('combobox', {name: '搜索网络节点'});
    const canvasBounds = await graph.locator('.network-canvas').boundingBox();
    await search.fill('找不到的节点');
    await graph.getByText('没有匹配的节点', {exact: true}).waitFor();
    assert.deepEqual(await graph.locator('.network-canvas').boundingBox(), canvasBounds, 'search dropdown does not resize or move the canvas');
    await search.press('Escape');
    assert.equal(await search.getAttribute('aria-expanded'), 'false');
    await search.fill('Electricity');
    // Utilities are hidden at this point, so this is also an empty search.
    await search.press('Escape');
    await search.fill('');
    await search.press('Escape');
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
    await graph.getByRole('combobox', {name: '搜索网络节点'}).fill('Electricity');
    await graph.locator('.search-results [role=option]').filter({hasText: '电力'}).first().click();
    await graph.getByRole('dialog', {name: '网络对象详情'}).waitFor();
    assert.equal(await graph.getByRole('dialog', {name: '网络对象详情'}).locator('[data-flow-kind="Electricity"]').count(), 1);
    await graph.getByRole('combobox', {name: '搜索网络节点'}).fill('铁板');
    await graph.locator('.search-results [role=option]').filter({hasText: '个机制'}).first().click();
    const card = graph.locator('.network-details [data-mechanic-editor]');
    await card.waitFor();
    assert.equal(await graph.locator('.flow-lanes').count(), 0, 'network editor only renders the shared left half');
    assert.equal(await graph.locator('.network-details').evaluate(node => getComputedStyle(node).position), 'absolute');
    await page.waitForTimeout(200); // finish the selection's short centering animation
    const popupPosition = () => graph.locator('.network-details').evaluate(node => ({left: node.offsetLeft, top: node.offsetTop}));
    const popupBefore = await popupPosition();
    await graph.locator('.network-canvas').evaluate(node => node._cyreg.cy.panBy({x: 0, y: 15}));
    await waitFor(async () => (await popupPosition()).top > popupBefore.top + 10);
    assert.equal(await card.getAttribute('data-mechanic-editor'), '1');
    await card.locator('.config-preview').click();
    const moduleMenu = page.getByRole('dialog', {name: '插件与插件塔', exact: true});
    await moduleMenu.waitFor();
    await moduleMenu.locator('[title="插件槽 1（点击更换）"]').click();
    await page.locator('.backdrop [aria-modal="true"]').waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await moduleMenu.count(), 1, 'closing the selector keeps the parent configuration menu');
    assert.equal(await card.count(), 1, 'closing the selector keeps the node popup');
    await page.keyboard.press('Escape');
    await moduleMenu.waitFor({state: 'hidden'});
    assert.equal(await card.count(), 1, 'Escape in a child menu does not close the node popup');
    await card.getByRole('switch', {name: '停用机制', exact: true}).click();
    await card.getByRole('switch', {name: '启用机制', exact: true}).waitFor();
    await ready();
    assert.ok((await page.evaluate(() => window.__layoutCalls.filter(c => c.command === 'dispatch')))
      .some(call => call.args.message.action.factory === 2 && call.args.message.action.action['mechanic-list']?.['set-enabled']?.mechanic === 1));
    await card.getByRole('switch', {name: '启用机制', exact: true}).click();
    await ready();
    // The second member must remain the editor target when an edit changes
    // its merged graph ID, and when it rejoins an existing group afterwards.
    await graph.locator('.variant-options').getByRole('button', {name: '机制 #2', exact: true}).click();
    const editor = await card.elementHandle();
    const editPrototype = async (button, name) => {
      await card.locator(button).click();
      const modal = page.locator('.backdrop [aria-modal="true"]');
      await modal.getByRole('textbox', {name: '搜索原型'}).fill(name);
      await modal.getByRole('button', {name, exact: true}).dblclick();
      await modal.waitFor({state: 'hidden'});
      await ready();
      assert.equal(await editor.evaluate(node => node.isConnected), true, 'editing does not remount the card');
      assert.equal(await card.getAttribute('data-mechanic-editor'), '2', 'keep the same mechanism, not the first group member');
      assert.equal(await graph.locator('.network-canvas').evaluate(node => {
        const cy = node._cyreg.cy;
        return cy.$(':selected').length === 1 && cy.$(':selected').first().isNode();
      }), true, 'selection follows the changed graph node');
    };
    await editPrototype('.primary', '钢材');
    await editPrototype('.machine-button', '锅炉');
    await editPrototype('.primary', '铁板');
    await editPrototype('.machine-button', '组装机 2 型');
    assert.equal(await graph.locator('.variant-options .active').textContent(), '机制 #2');
    await page.getByRole('button', {name: '列表', exact: true}).click();
    assert.equal(await graph.count(), 0);
    await page.locator('.center').evaluate(node => { node.scrollTop = 1200; });
    await page.waitForTimeout(100);
    await page.getByRole('button', {name: '网络', exact: true}).click();
    await ready();
    assert.equal(await graph.getByRole('combobox', {name: '搜索网络节点'}).inputValue(), '铁板');
    assert.ok(await graph.getByRole('checkbox', {name: '能量与污染', exact: true}).isChecked());
    assert.equal(await graph.getByLabel('网络显示大小', {exact: true}).inputValue(), '2');
    assert.equal(await graph.locator('.network-details [data-mechanic-editor]').count(), 1, 'selection survives switching views');
    assert.equal(await card.getAttribute('data-mechanic-editor'), '2', 'the edited group member survives switching views');
    await page.getByRole('button', {name: '列表', exact: true}).click();
    await waitFor(async () => Math.abs(await page.locator('.center').evaluate(node => node.scrollTop) - 1200) < 2);
    await page.getByRole('button', {name: '网络', exact: true}).click();
    await ready();
    await graph.getByRole('button', {name: '重排', exact: true}).click();
    await ready();
    if (await graph.getByRole('button', {name:'关闭网络详情', exact:true}).count()) await graph.getByRole('button', {name:'关闭网络详情', exact:true}).click();
    // Keep this repeated save/error test compact; large-size coverage lives in the universe regression.
    await graph.locator('.network-canvas').evaluate(el=>el._cyreg.cy.nodes().forEach((n,i)=>n.position({x:(i%15)*240,y:Math.floor(i/15)*240})));
    const exportPng = async (mode = 'success') => {
      if (process.env.NETWORK_TEST_TRACE) console.log(`export start: ${mode}`);
      await graph.getByRole('button', {name: '导出 PNG', exact: true}).click();
      await waitFor(async () => await page.evaluate(() => !!window.__finishPngSave));
      if (process.env.NETWORK_TEST_TRACE) console.log('save dialog pending');
      assert.ok(await graph.getByRole('button', {name: '导出中…', exact: true}).isDisabled());
      assert.match(await graph.locator('.export-status').textContent(), /请选择保存位置/);
      // Signature and dimensions are in the first 24 bytes; avoid serializing
      // the entire image through Playwright just to inspect its header.
      await page.evaluate(mode => window.__finishPngSave(mode), mode);
      await graph.getByRole('button', {name: '导出 PNG', exact: true}).waitFor();
      return Buffer.from(await page.evaluate(() => window.__pngExports.at(-1).bytes));
    };
    const bounds=await graph.locator('.network-canvas').evaluate(el=>{const b=el._cyreg.cy.elements().boundingBox();return {w:Math.ceil(b.w),h:Math.ceil(b.h),dpr:window.devicePixelRatio};});
    const png = await exportPng();
    assert.equal(bounds.dpr,2);
    assert.equal(png.readUInt32BE(16),bounds.w,'device pixel ratio must not double PNG width');
    assert.equal(png.readUInt32BE(20),bounds.h,'device pixel ratio must not double PNG height');
    assert.ok((await graph.locator('.export-status').textContent()).startsWith('PNG 已保存到：C:\\Exports\\metatorio-network-1-2.png'));
    assert.equal(png.subarray(1, 4).toString(), 'PNG');
    assert.ok(png.readUInt32BE(16) > 100 && png.readUInt32BE(20) > 100);
    await graph.locator('.network-canvas').hover({position: {x: 150, y: 150}});
    await page.mouse.wheel(0, -500);
    await page.waitForTimeout(200);
    const secondPng = await exportPng();
    assert.deepEqual([secondPng.readUInt32BE(16), secondPng.readUInt32BE(20)], [png.readUInt32BE(16), png.readUInt32BE(20)], 'whole-network export ignores viewport zoom');
    await exportPng('cancel');
    assert.equal(await graph.locator('.export-status').textContent(), '已取消导出');
    await exportPng('error');
    assert.match(await graph.getByRole('alert').textContent(), /PNG 导出失败.*模拟磁盘写入失败/);
    assert.equal(await graph.getByRole('button', {name: '重试布局', exact: true}).count(), 0, 'export errors do not claim the graph layout failed');
    await exportPng();
    assert.equal(await graph.getByRole('alert').count(), 0, 'export can be retried after a write failure');
    const aborts=await page.evaluate(()=>window.__pngAborts);
    await page.evaluate(()=>window.__holdPngTile=true);
    await graph.getByRole('button',{name:'导出 PNG',exact:true}).click();
    await waitFor(async()=>await page.evaluate(()=>!!window.__finishPngSave));
    await page.evaluate(()=>window.__finishPngSave('success'));
    await waitFor(async()=>await page.evaluate(()=>!!window.__releasePngTile));
    await graph.getByRole('button',{name:'取消导出',exact:true}).click();
    await page.evaluate(()=>{window.__holdPngTile=false;window.__releasePngTile();window.__releasePngTile=null;});
    await graph.getByRole('button',{name:'导出 PNG',exact:true}).waitFor();
    assert.equal(await graph.locator('.export-status').textContent(),'已取消导出');
    assert.equal(await page.evaluate(()=>window.__pngAborts),aborts+1);
    assert.equal(await page.evaluate(()=>window.__pngExports.at(-1).bytes.length),0);
    if (process.env.NETWORK_SCREENSHOT) {
      await graph.getByRole('button', {name: '定位匹配', exact: true}).click();
      await page.waitForTimeout(200);
      await page.screenshot({path: process.env.NETWORK_SCREENSHOT});
    }
    const canvasBeforeFilter = await graph.locator('.network-canvas').boundingBox();
    await page.getByRole('button', {name: '筛选', exact: true}).click();
    const filter = page.getByRole('region', {name: '机制流筛选'});
    await filter.waitFor();
    assert.deepEqual(await graph.locator('.network-canvas').boundingBox(), canvasBeforeFilter, 'floating filters do not squeeze or move the canvas');
    assert.equal(await graph.locator('.network-filter').evaluate(node => getComputedStyle(node).position), 'absolute');
    await filter.getByRole('button', {name: '选择产出对象', exact: true}).click();
    await filter.getByRole('textbox', {name: '搜索筛选对象'}).fill('铁板');
    await filter.locator('.flow-option').filter({has: page.locator('[data-flow-kind="Item"]')}).first().click();
    await filter.getByLabel('产出品质').selectOption('rare');
    await ready();
    await filter.getByLabel('产出品质').selectOption('legendary');
    await ready();
    assert.equal(await graph.locator('.network-canvas').count(), 1, 'zero matches retain the workspace and floating filter');
    await graph.getByText('没有匹配的机制，可更换条件或清除筛选。', {exact: true}).waitFor();
    await filter.getByLabel('产出品质').selectOption('rare');
    await page.getByRole('button', {name: '列表', exact: true}).click();
    await waitFor(async () => await page.locator('[data-mechanic-row]').count() === 1);
    assert.equal(await page.locator('[data-mechanic-row]').getAttribute('data-mechanic-row'), '2');
    await page.getByRole('button', {name: '网络', exact: true}).click();
    await ready();
    assert.equal(await filter.getByLabel('产出品质').inputValue(), 'rare');
    assert.ok((await graph.textContent()).includes('1 个机制节点'));
    await page.setViewportSize({width: 800, height: 700});
    await graph.getByRole('button', {name: '收起机制筛选', exact: true}).click();
    await search.fill('铁板');
    await search.press('Enter');
    const panel = graph.getByRole('dialog', {name: '网络对象详情'});
    await panel.waitFor();
    await waitFor(async () => {
      const stage = await graph.locator('.network-stage').boundingBox(), popup = await panel.boundingBox();
      return popup && popup.x >= stage.x && popup.y >= stage.y && popup.x + popup.width <= stage.x + stage.width + 1 && popup.y + popup.height <= stage.y + stage.height + 1;
    });
    await panel.getByRole('button', {name: '关闭网络详情', exact: true}).click();
    assert.equal(await panel.count(), 0);
    assert.equal(await graph.locator('.network-canvas').evaluate(node => node._cyreg.cy.$(':selected').length), 0, 'closing the editor clears graph selection');
    await page.waitForTimeout(200);
    await graph.locator('.network-canvas').evaluate(node => {node.scrollIntoView({block:'center'});const cy=node._cyreg.cy;cy.zoom(Math.max(.65,cy.zoom()));cy.center(cy.nodes('[kind="mechanic"]').first());});
    await page.waitForTimeout(100);
    const nodePoint=await graph.locator('.network-canvas').evaluate(node=>node._cyreg.cy.nodes('[kind="mechanic"]').first().renderedPosition());
    const visibleCanvas=await graph.locator('.network-canvas').boundingBox();
    await page.mouse.click(visibleCanvas.x+nodePoint.x,visibleCanvas.y+nodePoint.y);
    await panel.waitFor();
    assert.equal(await card.getAttribute('data-mechanic-editor'), '2', 'a real canvas node click opens the shared editor');
    await page.keyboard.press('Escape');
    await panel.waitFor({state: 'hidden'});
    assert.equal(await page.evaluate(() => document.body.scrollWidth > innerWidth), false);
    assert.ok(requests.every(request => request.startsWith(url) || request.startsWith('blob:') || request.startsWith('data:')), 'offline dependencies: no CDN requests');
    assert.deepEqual(errors, []);
    console.log('Network browser passed: lazy engines, full cached expansion/retry, canvas, shared editing/filter, state restoration, PNG, narrow viewport and offline assets.');
  } finally { await browser?.close(); server.kill(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
