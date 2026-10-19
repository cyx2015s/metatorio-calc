// Real Svelte lifecycle test with mocked IPC. Programmatic background clicks
// deliberately replace an open request to exercise the keyed session contract.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { seed, waitFor } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawn } = require('node:child_process');
const appRoot = path.resolve(__dirname, '..');
const port = Number(process.env.UI_TEST_PORT || 5200);
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
      window.__layoutDocument.projects[0].factories[0].targets = [
        {id: 1, flow: {Item: {id: 'iron-plate', quality: 'rare'}}, amount: 1},
        {id: 2, flow: {Item: {id: 'steel-plate', quality: 'normal'}}, amount: 2},
        {id: 3, flow: {Fluid: {name: 'steam', temperature: [165, 500]}}, amount: 3},
      ];
    });
    await page.goto(url);
    const targets = page.locator('.row-item').filter({has: page.locator('.flow-name')});
    await targets.first().waitFor();
    const modal = page.locator('.backdrop [aria-modal="true"]');
    const search = () => modal.getByRole('textbox', {name: '搜索原型'});
    const replaceRequest = locator => locator.evaluate(node => node.click());
    const selectedName = async () => modal.locator('.sg-items > button.active').getAttribute('aria-label');

    await targets.nth(0).locator('.hover-icon').click();
    await modal.waitFor();
    assert.equal(await modal.locator('.kind-tabs .active').textContent(), '物品');
    assert.equal(await modal.locator('.q-chip.active .q-name').textContent(), '稀有');
    assert.equal(await selectedName(), '铁板');
    await modal.getByTitle('选择实体类流').click();
    await modal.getByTitle('传说', {exact: true}).filter({has: page.locator('.q-name')}).click();
    await search().fill('草稿搜索');
    const oldFlowModal = await modal.elementHandle();
    // Refreshing catalog data must preserve the current draft and DOM instance.
    await page.evaluate(async () => {
      const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
      runtime.catalogIndex = JSON.parse(JSON.stringify(runtime.catalogIndex));
    });
    assert.equal(await oldFlowModal.evaluate(node => node.isConnected), true);
    assert.equal(await search().inputValue(), '草稿搜索');
    assert.equal(await modal.locator('.kind-tabs .active').textContent(), '实体');
    assert.equal(await modal.locator('.q-chip.active .q-name').textContent(), '传说');

    await replaceRequest(targets.nth(1).locator('.hover-icon'));
    await waitFor(async () => !(await oldFlowModal.evaluate(node => node.isConnected)));
    assert.equal(await search().inputValue(), '');
    assert.equal(await modal.locator('.kind-tabs .active').textContent(), '物品');
    assert.equal(await modal.locator('.q-chip.active .q-name').textContent(), '普通');
    assert.equal(await selectedName(), '钢材');
    const secondFlowModal = await modal.elementHandle();
    await replaceRequest(targets.nth(2).locator('.hover-icon'));
    await waitFor(async () => !(await secondFlowModal.evaluate(node => node.isConnected)));
    assert.equal(await modal.locator('.kind-tabs .active').textContent(), '流体');
    assert.equal(await selectedName(), '蒸汽');
    assert.equal(await modal.locator('.q-chip').count(), 0);
    await page.keyboard.press('Escape');
    await targets.nth(0).locator('.hover-icon').click();
    assert.equal(await modal.locator('.q-chip.active .q-name').textContent(), '稀有');
    assert.equal(await selectedName(), '铁板');
    await page.keyboard.press('Escape');

    // Catalog requests have the same lifecycle, including a category change.
    await page.locator('[data-mechanic-id="1"] .primary').click();
    await modal.waitFor();
    assert.equal(await selectedName(), '铁板');
    await modal.locator('.sg-items > button.active').click();
    await modal.getByTitle('传说', {exact: true}).filter({has: page.locator('.q-name')}).click();
    await search().fill('草稿');
    const oldCatalogModal = await modal.elementHandle();
    await replaceRequest(page.locator('[data-mechanic-id="1"] .machine-button'));
    await waitFor(async () => !(await oldCatalogModal.evaluate(node => node.isConnected)));
    assert.equal(await search().inputValue(), '');
    assert.equal(await modal.locator('.kind-tabs .active').textContent(), '制造机');
    assert.equal(await modal.locator('.q-chip.active .q-name').textContent(), '普通');
    assert.equal(await selectedName(), '组装机 2 型');
    // The same editor policy can be used outside cards. A delayed picker must
    // not open in a different factory; an existing callback retains its target.
    const shared = await page.evaluate(async () => {
      const {createMechanicActions} = await import('/src/lib/mechanics/actions.ts');
      const {MechanicCommands} = await import('/src/lib/runtime/mechanic-commands.ts');
      const document = structuredClone(window.__layoutDocument), requests = [], messages = [];
      const host = {
        document, selectedProjectId: 1, selectedFactoryId: 2, effectiveContextId: 'test',
        getDetail: async () => ({categories: ['crafting']}),
        localizedName: (_kind, name) => name,
        mechanicCommands(scope) {
          return new MechanicCommands({
            send: async message => { messages.push(message); },
            getFactory: ({project, factory}) => document.projects.find(p => p.id === project)?.factories.find(f => f.id === factory) ?? null,
          }, scope);
        },
      };
      const actions = createMechanicActions(host, {project: 1, factory: 2}, {
        select: request => requests.push(request), selectFlow: request => requests.push(request), notice() {},
      });
      await actions.pick(15, 'tile');
      const extract = requests.at(-1);
      await actions.pick(16, 'tile');
      const dispose = requests.at(-1);
      host.selectedFactoryId = 999;
      await extract.onSelect('water', 'normal');
      await dispose.onSelect('lava', 'normal');
      host.selectedFactoryId = 2;
      let resolveDetail;
      host.getDetail = () => new Promise(resolve => { resolveDetail = resolve; });
      const pending = actions.pick(1, 'machine');
      host.selectedFactoryId = 999;
      resolveDetail({categories: ['crafting']});
      await pending;
      return {purposes: [extract.tilePurpose, dispose.tilePurpose], requests: requests.length, messages};
    });
    assert.deepEqual(shared.purposes, ['extract', 'dispose']);
    assert.equal(shared.requests, 2, 'a stale asynchronous picker must not open');
    assert.ok(shared.messages.every(message => message.action.project === 1 && message.action.factory === 2));
    assert.deepEqual(shared.messages.map(message => Object.keys(message.action.action.mechanic.action)[0]), ['tile-extract', 'tile-dispose']);
    assert.deepEqual(errors, []);
    console.log('Selector sessions passed: draft lifecycle, shared editor policies, tile purposes, scoped callbacks and stale asynchronous requests.');
  } finally {
    await browser?.close();
    server.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
