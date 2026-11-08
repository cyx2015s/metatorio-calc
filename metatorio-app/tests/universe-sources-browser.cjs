const {withBrowser, waitFor} = require('./browser-harness.cjs');
const {seed} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({port: Number(process.env.UI_TEST_PORT || 5228)}, async (browser, url) => {
  const page = await browser.newPage({viewport: {width: 1600, height: 1000}}), errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await seed(page, 1);
  await page.addInitScript(() => {
    const p = window.__layoutDocument.projects[0];
    const located = (i, name) => ({location: {kind: 'planet', name}, flow: {Custom: {name: `source-${i}`}}});
    const sources = Array.from({length: 300}, (_, i) => [located(i, 'nauvis'), located(i, 'fulgora')]).flat();
    p.universes = [{id: 9, name: 'Large sources', mechanics: [], targets: [], target_expressions: [],
      external_inputs: Array.from({length: 300}, (_, i) => ({id: i + 100, flow: located(i, 'nauvis'), penalty: 1})), strict_source: true, strict_sink: false}];
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__sourceKeys = 0; window.__sourcePreviews = 0;
    const stringify = JSON.stringify;
    JSON.stringify = function(value, ...rest) {
      if (Array.isArray(value) && value.length === 2 && typeof value[0] === 'string' && value[0].startsWith('planet:')) window.__sourceKeys++;
      return stringify.call(this, value, ...rest);
    };
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'universe_preview') {window.__sourcePreviews++; return {mechanics: [], sources};}
      if (command === 'solve_universe') return {project: 1, universe: 9, status: {status: 'solved', cost: 0, mechanics: [], flows: []}};
      if (command === 'dispatch') {
        const a = args.message.action.action;
        if (a['set-time-scale']) p.settings.time_scale = a['set-time-scale'].time_scale;
        if (a['set-mining-productivity']) p.settings.mining_productivity = a['set-mining-productivity'].productivity;
        if (a['set-name']) p.name = a['set-name'].name;
        return {revision: 1, changed: true, commands: [], created: {}, history: {can_undo: true, can_redo: false}};
      }
      return invoke(command, args);
    };
  });
  await page.goto(url); await page.locator('.mech-card').first().waitFor();
  await page.getByRole('button', {name: '列表', exact: true}).click();
  await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    const load = runtime.loadUniversePreview.bind(runtime);
    window.__sourceReads = 0;
    runtime.loadUniversePreview = (...args) => {window.__sourceReads++; return load(...args);};
    window.__sourceKeys = 0;
    await runtime.selectUniverse(9);
  });
  const implicit = page.locator('.inputs-panel .implicit');
  await waitFor(async () => await implicit.count() === 300);
  const initialKeys = await page.evaluate(() => window.__sourceKeys);
  assert.ok((await implicit.first().textContent()).includes('source-0'));
  const metrics = await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    const paint = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    let before = window.__sourceReads;
    await runtime.setTimeScale('hours'); await paint();
    await runtime.setProjectName('Renamed'); await paint();
    window.__layoutDocument.projects[0].universes[0].external_inputs.reverse();
    await runtime.refresh(); await paint();
    const displayReads = window.__sourceReads - before;
    before = window.__sourcePreviews;
    await runtime.setMiningProductivity(0.5); await paint();
    const numericRequests = window.__sourcePreviews - before;
    before = window.__sourcePreviews;
    runtime.clearMechanicFlowCache(); await paint();
    return {displayReads, numericRequests, reloadRequests: window.__sourcePreviews - before};
  });
  console.log(JSON.stringify({initialKeys, ...metrics}));
  assert.ok(initialKeys < 10000, `matching must be linear, observed ${initialKeys} serializations`);
  assert.deepEqual(metrics, {displayReads: 0, numericRequests: 1, reloadRequests: 1});
  assert.equal(await implicit.count(), 300, 'same flow on another planet stays implicit');
  assert.deepEqual(errors, []);
  console.log('Universe sources passed: linear matching, display/order reuse, physical invalidation and explicit context reload.');
}).catch(error => {console.error(error); process.exitCode = 1;});
