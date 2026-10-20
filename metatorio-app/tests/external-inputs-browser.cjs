const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const {seed, waitFor} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const {spawn} = require('node:child_process');
const appRoot = path.resolve(__dirname, '..'), port = Number(process.env.UI_TEST_PORT || 5204);
const url = `http://127.0.0.1:${port}`;
(async () => {
  const server = spawn(process.execPath, [path.join(appRoot, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], {cwd: appRoot, windowsHide: true, stdio: 'ignore'});
  let browser;
  try {
    await waitFor(async () => { try { return (await fetch(url)).ok; } catch { return false; } }, 600);
    browser = await chromium.launch({headless: true, ...(process.env.BROWSER_EXECUTABLE ? {executablePath: process.env.BROWSER_EXECUTABLE} : {})});
    const page = await browser.newPage({viewport: {width: 1600, height: 1100}});
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    await seed(page, 16);
    await page.addInitScript(() => {
      const factory = window.__layoutDocument.projects[0].factories[0];
      factory.external_inputs.push({id: 300, flow: 'Electricity', penalty: 123});
      const resources = [{Entity: {id: 'coal', quality: 'normal'}}, {Fluid: {name: 'water', temperature: [15, 15]}}];
      const raw = window.__TAURI_INTERNALS__.invoke;
      let nextId = 301;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'implicit_sources') return resources.filter(flow => !factory.external_inputs.some(input => JSON.stringify(input.flow) === JSON.stringify(flow)));
        const value = await raw(command, args);
        if (command === 'dispatch') {
          const action = args.message.action?.action?.['external-input'];
          // Match Tauri's JSON boundary: do not retain Svelte proxy objects in
          // the mock document, which get_document later structured-clones.
          if (action?.add) factory.external_inputs.push({...JSON.parse(JSON.stringify(action.add.input)), id: nextId++});
          if (action?.['set-penalty']) factory.external_inputs.find(input => input.id === action['set-penalty'].input).penalty = action['set-penalty'].penalty;
          if (action?.remove) factory.external_inputs.splice(factory.external_inputs.findIndex(input => input.id === action.remove.input), 1);
        }
        return value;
      };
    });
    await page.goto(url);
    const panel = page.getByRole('region', {name: '外部输入', exact: true});
    const freeEnergy = panel.getByRole('button', {name: '+ 免费电力与热量', exact: true});
    const inputs = () => page.evaluate(() => window.__layoutDocument.projects[0].factories[0].external_inputs);
    await freeEnergy.click();
    await waitFor(async () => await freeEnergy.isDisabled());
    assert.deepEqual((await inputs()).map(({id, flow, penalty}) => ({id, flow, penalty})), [
      {id: 300, flow: 'Electricity', penalty: 0}, {id: 301, flow: 'Heat', penalty: 0},
    ]);
    const manualRows = panel.locator('.rows').first().locator('.row-item');
    await manualRows.filter({has: page.locator('[data-flow-kind="Heat"]')}).getByRole('button', {name: '移除外部输入', exact: true}).click();
    await waitFor(async () => !(await freeEnergy.isDisabled()));
    await freeEnergy.click();
    await waitFor(async () => (await inputs()).length === 2 && await freeEnergy.isDisabled());
    assert.equal((await inputs()).filter(input => input.flow === 'Electricity').length, 1);

    await panel.getByRole('button', {name: /^禁用环境资源：.*煤炭/}).click();
    await waitFor(async () => (await inputs()).some(input => input.flow?.Entity?.id === 'coal' && input.penalty === 1e12));
    await waitFor(async () => await panel.getByRole('button', {name: /^撤销禁用：.*煤炭/}).count() === 1).catch(async error => {
      throw Error(`${error}\n${JSON.stringify({errors, inputs: await inputs(), text: await panel.textContent(), buttons: await panel.locator('button').evaluateAll(nodes => nodes.map(node => [node.textContent, node.getAttribute('aria-label')])), calls: await page.evaluate(() => window.__layoutCalls.slice(-5))})}`);
    });
    assert.equal(await panel.getByRole('button', {name: /^禁用环境资源：.*煤炭/}).count(), 0);
    assert.ok((await manualRows.filter({hasText: '煤炭'}).textContent()).includes('已禁用'));
    await panel.getByRole('button', {name: /^撤销禁用：.*煤炭/}).click();
    await panel.getByRole('button', {name: /^禁用环境资源：.*煤炭/}).waitFor();
    assert.equal((await inputs()).some(input => input.flow?.Entity?.id === 'coal'), false);

    // Display time scale is minutes: disabling still sends the same internal cost.
    await panel.getByRole('button', {name: /^禁用环境资源：.*水/}).click();
    await waitFor(async () => (await inputs()).some(input => input.flow?.Fluid?.name === 'water' && input.penalty === 1e12));
    const coalRow = panel.locator('.row-item.implicit').filter({hasText: '煤炭'});
    await coalRow.getByRole('button', {name: '设为手动外部输入', exact: true}).click();
    await waitFor(async () => (await inputs()).some(input => input.flow?.Entity?.id === 'coal' && input.penalty === 0));
    assert.equal(await panel.getByRole('button', {name: /^撤销禁用：.*煤炭/}).count(), 0);
    const calls = await page.evaluate(() => window.__layoutCalls.filter(call => call.command === 'dispatch'));
    assert.ok(calls.every(call => call.args.message.scope === 'factory' && call.args.message.action.project === 1 && call.args.message.action.factory === 2 && call.args.message.action.action['external-input']));
    await page.setViewportSize({width: 800, height: 700});
    assert.equal(await page.evaluate(() => document.body.scrollWidth > innerWidth), false);
    assert.deepEqual(errors, []);
    console.log('External input shortcuts passed: zero-cost energy, reuse, suppression, restore, unit independence, implicit promotion and existing scoped messages only.');
  } finally { await browser?.close(); server.kill(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
