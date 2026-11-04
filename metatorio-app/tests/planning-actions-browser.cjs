const {withBrowser, waitFor} = require('./browser-harness.cjs');
const {seed} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({port: Number(process.env.UI_TEST_PORT || 5225)}, async (browser, url) => {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await seed(page, 1);
  await page.addInitScript(() => {
    const doc = window.__layoutDocument;
    doc.projects.push({...structuredClone(doc.projects[0]), id: 50, factories: [], name: 'Other project'});
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__beaconRequests = [];
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command !== 'dispatch') return invoke(command, args);
      const message = JSON.parse(JSON.stringify(args.message));
      return new Promise(resolve => window.__beaconRequests.push({message, release() {
        const action = message.action.action.planning;
        const beacon = action['add-enumerated-beacon-with-prototype']?.beacon;
        if (beacon) doc.projects.find(p => p.id === message.action.project).planning.enumerate_beacons.push({
          module_config: {modules: [], beacons: [{beacon, count: 1, share: 1, modules: []}]},
        });
        resolve({revision: 1, changed: true, created: {}, commands: [], history: {can_undo: true, can_redo: false}});
      }}));
    };
  });
  await page.goto(url);
  await page.locator('.mech-card').waitFor();
  await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    window.__addingBeacons = Promise.all([
      runtime.addEnumeratedBeacon({id: 'beacon', quality: 'rare'}),
      runtime.addEnumeratedBeacon({id: 'beacon', quality: 'legendary'}),
    ]);
    await runtime.selectProject(50);
  });
  await waitFor(() => page.evaluate(() => window.__beaconRequests.length === 2));
  // Finish out of order, after selection changed; each request owns its complete plan.
  await page.evaluate(() => {
    window.__beaconRequests[1].release();
    window.__beaconRequests[0].release();
  });
  const result = await page.evaluate(async () => {
    await window.__addingBeacons;
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    return {messages: window.__beaconRequests.map(r => r.message), projects: runtime.document.projects, selected: runtime.selectedProjectId, busy: runtime.busy};
  });
  assert.equal(result.messages.length, 2, 'one dispatch per complete beacon plan');
  assert.ok(result.messages.every(m => m.scope === 'project' && m.action.project === 1));
  assert.deepEqual(result.projects[0].planning.enumerate_beacons.map(p => p.module_config.beacons[0].beacon.quality), ['legendary', 'rare']);
  assert.equal(result.projects[1].planning.enumerate_beacons.length, 0);
  assert.equal(result.selected, 50);
  assert.equal(result.busy, false);
  assert.deepEqual(errors, []);
  console.log('Beacon plans passed: one dispatch per click, concurrent completion and project selection isolation.');
}).catch(error => {console.error(error); process.exitCode = 1;});
