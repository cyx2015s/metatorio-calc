const { withBrowser, waitFor } = require('./browser-harness.cjs');
const { seed } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({ port: 5221 }, async (browser, url) => {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await seed(page, 5);
  await page.addInitScript(() => {
    const doc = window.__layoutDocument;
    Object.assign(doc.projects[0].settings, {
      reactor_neighbours: { 'nuclear-reactor': 0, 'missing-mod-reactor': 255 },
      orbital_transport_costs: { rocket_export_per_kg: 1, orbital_drop_per_stack: 64, orbital_drop_spoil_penalty: 216000 },
    });
    const other = structuredClone(doc.projects[0]);
    other.id = 10; other.name = '另一个项目'; other.factories[0].id = 20;
    doc.projects.push(other);
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__estimateEdits = [];
    window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
      const action = args.message?.action?.action;
      if (command === 'dispatch' && (action?.['set-reactor-neighbours'] || action?.['set-orbital-transport-costs'])) {
        window.__estimateEdits.push(structuredClone(args.message));
        if (window.__estimateFail) throw '保存失败（测试）';
        if (window.__estimateDelay) await new Promise(resolve => window.__estimateRelease = resolve);
        const project = doc.projects.find(p => p.id === args.message.action.project);
        if (action['set-reactor-neighbours']) project.settings.reactor_neighbours = action['set-reactor-neighbours'].estimates;
        else project.settings.orbital_transport_costs = action['set-orbital-transport-costs'].costs;
        return { revision: window.__estimateEdits.length + 1, changed: true, created: {}, commands: [], history: { can_undo: true, can_redo: false } };
      }
      if (command === 'catalog_index') {
        const index = await invoke(command, args);
        if (!index.entries.some(entry => entry.kind === 'fusion-reactor')) {
          index.entries.push({ ...index.entries.find(entry => entry.kind === 'reactor'), kind: 'fusion-reactor', name: 'fusion-reactor', localized_name: '聚变反应堆' });
        }
        return index;
      }
      return invoke(command, args);
    };
  });
  await page.goto(url);
  await page.getByRole('tab', { name: /^项目/ }).click();
  const panel = page.getByRole('region', { name: '项目估算设置' });
  const cost = page.getByRole('spinbutton', { name: '火箭出口成本', exact: true });
  const edit = async (input, value) => { await input.fill(value); await input.press('Tab'); };
  const edits = () => page.evaluate(() => window.__estimateEdits.length);
  assert.equal(await cost.inputValue(), '1');
  assert.equal(await page.getByRole('spinbutton', { name: '邻接数量：nuclear-reactor', exact: true }).inputValue(), '0');
  for (const invalid of ['', '-1', '1e309']) {
    await edit(cost, invalid);
    assert.equal(await edits(), 0, `invalid cost ${invalid}`);
    assert.equal(await cost.inputValue(), '1');
  }
  for (const invalid of ['-1', '1.5', '256', '']) {
    await edit(page.getByRole('spinbutton', { name: '邻接数量：nuclear-reactor', exact: true }), invalid);
    assert.equal(await edits(), 0, `invalid neighbour ${invalid}`);
  }
  await edit(cost, '0');
  await waitFor(async () => await edits() === 1 && await cost.isEnabled());
  assert.deepEqual(await page.evaluate(() => window.__layoutDocument.projects[0].settings.orbital_transport_costs), {
    rocket_export_per_kg: 0, orbital_drop_per_stack: 64, orbital_drop_spoil_penalty: 216000,
  });
  await edit(page.getByRole('spinbutton', { name: '轨道投送成本', exact: true }), '1e6');
  await waitFor(async () => await edits() === 2 && await cost.isEnabled());
  await edit(page.getByRole('spinbutton', { name: '变质额外成本系数', exact: true }), '0.25');
  await waitFor(async () => await edits() === 3 && await cost.isEnabled());
  await edit(page.getByRole('spinbutton', { name: '邻接数量：nuclear-reactor', exact: true }), '255');
  await waitFor(async () => await edits() === 4 && await cost.isEnabled());
  await page.getByRole('button', { name: '恢复默认邻接数量：missing-mod-reactor', exact: true }).click();
  await waitFor(async () => await edits() === 5 && await cost.isEnabled());
  assert.equal(await page.getByRole('spinbutton', { name: '邻接数量：missing-mod-reactor', exact: true }).count(), 0);
  await page.getByRole('button', { name: '添加反应堆估算…' }).click();
  await page.getByRole('button', { name: '取消', exact: true }).click();
  assert.equal(await edits(), 5, 'cancel does not create overrides');
  await page.getByRole('button', { name: '恢复默认邻接数量：nuclear-reactor', exact: true }).click();
  await waitFor(async () => await edits() === 6 && await cost.isEnabled());
  await page.getByRole('button', { name: '添加反应堆估算…' }).click();
  await page.getByTitle('选择：核反应堆（nuclear-reactor）', { exact: true }).dblclick();
  await waitFor(async () => await edits() === 7 && await cost.isEnabled());
  assert.equal(await page.getByRole('spinbutton', { name: '邻接数量：nuclear-reactor', exact: true }).inputValue(), '3');
  await page.getByRole('button', { name: '添加反应堆估算…' }).click();
  await page.getByTitle('浏览聚变反应堆目录', { exact: true }).click();
  await page.getByTitle('选择：聚变反应堆（fusion-reactor）', { exact: true }).dblclick();
  await waitFor(async () => await edits() === 8 && await cost.isEnabled());
  assert.equal(await page.getByRole('spinbutton', { name: '邻接数量：fusion-reactor', exact: true }).inputValue(), '3');
  await page.getByRole('button', { name: '添加反应堆估算…' }).click();
  await page.getByTitle('选择：核反应堆（nuclear-reactor）', { exact: true }).dblclick();
  assert.equal(await edits(), 8, 'selecting an existing override preserves its value');
  await page.evaluate(() => window.__estimateFail = true);
  await edit(cost, '9');
  await page.getByRole('alert').filter({ hasText: '保存失败（测试）' }).waitFor();
  assert.equal(await cost.inputValue(), '0', 'failed save restores committed value');
  await page.evaluate(() => { window.__estimateFail = false; window.__estimateDelay = true; });
  await edit(cost, '2');
  await waitFor(async () => await cost.isDisabled());
  assert.equal(await panel.getByRole('button', { name: '添加反应堆估算…' }).isDisabled(), true);
  // Switch selection while the old project's request remains in flight.
  await page.evaluate(async () => {
    const { runtime } = await import('/src/lib/runtime/store.svelte.ts');
    await runtime.selectProject(10);
  });
  await waitFor(async () => await cost.inputValue() === '1');
  await page.evaluate(() => window.__estimateRelease());
  await waitFor(async () => await page.evaluate(() => window.__layoutDocument.projects[0].settings.orbital_transport_costs.rocket_export_per_kg === 2));
  assert.equal(await cost.inputValue(), '1', 'old reply does not overwrite the new project');
  assert.equal(await page.getByRole('alert').filter({ hasText: '保存失败（测试）' }).count(), 0);
  await page.evaluate(() => window.__estimateDelay = false);
  await page.getByRole('button', { name: '恢复全部默认邻接数量', exact: true }).click();
  await waitFor(async () => await panel.getByRole('spinbutton', { name: /^邻接数量/ }).count() === 0);
  assert.deepEqual(await page.evaluate(() => window.__layoutDocument.projects[1].settings.reactor_neighbours), {});
  const beforeLegacyRead = await edits();
  await page.evaluate(async () => {
    delete window.__layoutDocument.projects[1].settings.orbital_transport_costs;
    delete window.__layoutDocument.projects[1].settings.reactor_neighbours;
    const { runtime } = await import('/src/lib/runtime/store.svelte.ts');
    await runtime.refresh();
  });
  await waitFor(async () => await cost.inputValue() === '262144');
  assert.equal(await page.getByRole('spinbutton', { name: '轨道投送成本', exact: true }).inputValue(), '64');
  assert.equal(await page.getByRole('spinbutton', { name: '变质额外成本系数', exact: true }).inputValue(), '262144');
  assert.equal(await edits(), beforeLegacyRead, 'reading missing fields does not write defaults');
  await edit(cost, '0');
  await waitFor(async () => await edits() === beforeLegacyRead + 1 && await cost.isEnabled());
  assert.deepEqual(await page.evaluate(() => window.__layoutDocument.projects[1].settings.orbital_transport_costs), {
    rocket_export_per_kg: 0, orbital_drop_per_stack: 64, orbital_drop_spoil_penalty: 262144,
  });
  await panel.scrollIntoViewIfNeeded();
  if (process.env.ESTIMATES_SCREENSHOT) await page.screenshot({ path: process.env.ESTIMATES_SCREENSHOT });
  assert.equal(await panel.evaluate(el => el.scrollWidth > el.clientWidth), false);
  assert.deepEqual(errors, []);
  console.log('Project estimates: bounds, zero, scientific notation, sibling preservation, missing prototypes, cancel, failure and project-switch isolation passed.');
}).catch(error => { console.error(error); process.exitCode = 1; });
