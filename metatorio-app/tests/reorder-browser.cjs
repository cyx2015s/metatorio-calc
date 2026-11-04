const {withBrowser, waitFor} = require('./browser-harness.cjs');
const {seed} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({port: Number(process.env.UI_TEST_PORT || 5227)}, async (browser, url) => {
  const page = await browser.newPage(); const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await seed(page, 5);
  await page.addInitScript(() => {
    const project = window.__layoutDocument.projects[0], first = project.factories[0];
    first.external_inputs = [11, 12, 13, 14].map(id => ({id, flow: 'Electricity', penalty: id}));
    project.factories.push({...structuredClone(first), id: 3, name: 'Other factory'});
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__reorderWrites = [];
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      const move = command === 'dispatch' && args.message.action?.action?.['external-input']?.reorder;
      if (!move) return invoke(command, args);
      window.__reorderWrites.push(JSON.parse(JSON.stringify(args.message)));
      if (window.__reorderWrites.length === 1) await new Promise(resolve => window.__releaseReorder = resolve);
      const factory = project.factories.find(f => f.id === args.message.action.factory);
      const entries = factory.external_inputs;
      entries.splice(move.position, 0, ...entries.splice(entries.findIndex(e => e.id === move.input), 1));
      return {revision: window.__reorderWrites.length, changed: true, commands: [], created: {}, history: {can_undo: true, can_redo: false}};
    };
  });
  await page.goto(url); await page.locator('.mech-card').first().waitFor();
  await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    window.__reordering = runtime.reorderExternalInputs([14, 13, 12, 11]);
  });
  await waitFor(() => page.evaluate(() => typeof window.__releaseReorder === 'function'));
  await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    await runtime.selectFactory(3);
    window.__releaseReorder();
    await window.__reordering;
  });
  const reordered = await page.evaluate(() => ({writes: window.__reorderWrites,
    lists: window.__layoutDocument.projects[0].factories.map(f => f.external_inputs.map(i => i.id))}));
  assert.deepEqual(reordered.lists, [[14, 13, 12, 11], [11, 12, 13, 14]]);
  assert.ok(reordered.writes.every(m => m.action.project === 1 && m.action.factory === 2));
  const count = await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    const commands = runtime.mechanicCommands({project: 1, factory: 2});
    const before = window.__layoutCalls.filter(c => c.command === 'dispatch').length;
    await commands.reorderMechanics([5, 1, 2, 3, 4]);
    await commands.reorderMechanics([5, 1, 2, 3, 4]);
    return {writes: window.__layoutCalls.filter(c => c.command === 'dispatch').length - before,
      ids: window.__layoutDocument.projects[0].factories[0].mechanics.map(m => m.id)};
  });
  assert.deepEqual(count, {writes: 1, ids: [5, 1, 2, 3, 4]});
  assert.deepEqual(errors, []);
  console.log('Reorder passed: ordinary input scope survives awaits, single-mechanism drag uses one write and unchanged order uses none.');
}).catch(error => {console.error(error); process.exitCode = 1;});
