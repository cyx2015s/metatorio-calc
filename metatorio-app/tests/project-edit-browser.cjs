const { withBrowser, waitFor } = require('./browser-harness.cjs');
const { seed } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({ port: 5224 }, async (browser, url) => {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await seed(page, 160);
  await page.addInitScript(() => {
    const doc = window.__layoutDocument, p = doc.projects[0];
    p.factories.push({ ...structuredClone(p.factories[0]), id: 3, name: 'empty', mechanics: [] });
    p.universes = [{ id: 9, name: 'universe', mechanics: [], targets: [], external_inputs: [], target_expressions: [], strict_source: true, strict_sink: false }];
    const invoke = window.__TAURI_INTERNALS__.invoke;
    let revision = 1;
    window.__previewRequests = [];
    window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
      if (command === 'get_document' && window.__delayDocument) {
        window.__delayDocument = false;
        const old = structuredClone(doc);
        return new Promise(resolve => window.__releaseDocument = () => resolve(old));
      }
      if (command === 'universe_preview') return new Promise((resolve, reject) => window.__previewRequests.push({ resolve, reject }));
      if (command === 'dispatch') {
        const action = args.message.action.action;
        for (const [key, field, value] of [
          ['set-name', 'name', action['set-name']?.name],
          ['set-time-scale', 'time_scale', action['set-time-scale']?.time_scale],
          ['set-mining-productivity', 'mining_productivity', action['set-mining-productivity']?.productivity],
        ]) if (key in action) (field === 'name' ? p : p.settings)[field] = value;
        return { revision: ++revision, changed: true, created: {}, commands: [], history: { can_undo: true, can_redo: false } };
      }
      return invoke(command, args);
    };
  });
  await page.goto(url);
  await page.locator('.mech-card').first().waitFor();
  await waitFor(async () => (await page.locator('.flow-chip').count()) > 0);
  const result = await page.evaluate(async () => {
    const { runtime } = await import('/src/lib/runtime/store.svelte.ts');
    const paint = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const flows = () => window.__layoutCalls.filter(c => c.command === 'mechanic_flow').length;
    const before = runtime.document, factories = runtime.selectedProject.factories, mechanics = runtime.selectedFactory.mechanics;
    const solve = runtime.solve;
    let count = flows();
    await runtime.setProjectName('renamed'); await paint();
    const name = { same: factories === runtime.selectedProject.factories, solve: solve === runtime.solve, flows: flows() - count };
    count = flows();
    await runtime.setTimeScale('hours'); await paint();
    const display = { same: mechanics === runtime.selectedFactory.mechanics, flows: flows() - count };
    count = flows();
    await runtime.setMiningProductivity(0.5); await paint();
    const numeric = { same: mechanics === runtime.selectedFactory.mechanics, invalidated: runtime.solve === null, flows: flows() - count };
    count = flows();
    runtime.clearMechanicFlowCache(); await paint();
    const reload = flows() - count;
    const oldValue = before.projects[0].settings.mining_productivity;
    // One changed mechanic replaces its path, retaining siblings and the empty factory.
    const previous = runtime.selectedFactory.mechanics;
    window.__layoutDocument.projects[0].factories[0].mechanics[0].mechanic.recipe.id = 'steel-plate';
    await runtime.refresh(); await paint();
    const changed = { array: previous !== runtime.selectedFactory.mechanics,
      entry: previous[0] !== runtime.selectedFactory.mechanics[0], sibling: previous[1] === runtime.selectedFactory.mechanics[1],
      factory: factories[1] === runtime.selectedProject.factories[1] };
    // A superseded refresh cannot publish its older snapshot.
    window.__delayDocument = true;
    const first = runtime.refresh();
    window.__layoutDocument.projects[0].name = 'latest';
    const second = runtime.refresh();
    window.__releaseDocument();
    await Promise.all([first, second]);
    return { name, display, numeric, reload, oldValue, changed, latest: runtime.selectedProject.name };
  });
  assert.deepEqual(result.name, { same: true, solve: true, flows: 0 });
  assert.deepEqual(result.display, { same: true, flows: 0 });
  assert.equal(result.numeric.same, true);
  assert.equal(result.numeric.invalidated, true);
  assert.ok(result.numeric.flows > 0, 'numerical changes still reload visible coefficients');
  assert.ok(result.reload > 0, 'context cache invalidation still reloads visible coefficients');
  assert.equal(result.oldValue, 0, 'published snapshots are not mutated');
  assert.deepEqual(result.changed, { array: true, entry: true, sibling: true, factory: true });
  assert.equal(result.latest, 'latest');
  const cache = await page.evaluate(async () => {
    const { runtime } = await import('/src/lib/runtime/store.svelte.ts');
    const old = runtime.loadUniversePreview(1, 9).catch(() => 'old failure');
    window.__layoutDocument.projects[0].settings.mining_productivity = 1;
    await runtime.refresh();
    const current = runtime.loadUniversePreview(1, 9);
    const reused = runtime.loadUniversePreview(1, 9) === current;
    window.__previewRequests[0].reject('old failure');
    await old;
    const afterFailure = runtime.loadUniversePreview(1, 9) === current;
    window.__previewRequests[1].resolve({ mechanics: [], implicit_sources: [] });
    await current;
    return { reused, afterFailure, requests: window.__previewRequests.length };
  });
  assert.deepEqual(cache, { reused: true, afterFailure: true, requests: 2 });
  assert.deepEqual(errors, []);
  console.log('Project edit regression passed: shared identities, display-only reuse, physical invalidation, context reload, immutable old snapshots, stale refresh isolation and preview failure ownership.');
}).catch(error => { console.error(error); process.exitCode = 1; });
