const {withBrowser} = require('./browser-harness.cjs');
const {seed} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');

withBrowser({port: Number(process.env.UI_TEST_PORT || 5226)}, async (browser, url) => {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await seed(page, 1);
  await page.goto(url);
  await page.locator('.mech-card').waitFor();
  const result = await page.evaluate(async () => {
    const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
    const invoke = window.__TAURI_INTERNALS__.invoke, pending = [];
    window.__TAURI_INTERNALS__.invoke = (command, args) => command === 'catalog_index'
      ? new Promise((resolve, reject) => pending.push({resolve, reject})) : invoke(command, args);
    const index = marker => ({context_id: 'test', qualities: ['normal'], names: {marker}, entries: []});
    runtime.clearCatalogCache();
    const old = runtime.loadCatalogIndex();
    runtime.clearCatalogCache();
    const fresh = runtime.loadCatalogIndex();
    const requestsAfterClear = pending.length;
    // Avoid hanging on a broken implementation that reused the old request.
    if (requestsAfterClear !== 2) return {requestsAfterClear};
    const iconRevision = runtime.iconRevision;
    pending[0].resolve(index('old'));
    await old;
    const stalePublished = runtime.catalogIndex !== null;
    const shared = runtime.loadCatalogIndex();
    const requestsAfterOld = pending.length;
    pending[1].resolve(index('fresh'));
    await Promise.all([fresh, shared]);
    const first = {marker: runtime.catalogIndex.names.marker, icons: runtime.iconRevision - iconRevision};

    runtime.clearCatalogCache();
    const staleFailure = runtime.loadCatalogIndex();
    runtime.clearCatalogCache();
    const replacement = runtime.loadCatalogIndex();
    const previousError = runtime.lastError;
    pending[2].reject('stale catalog failure');
    await staleFailure;
    const staleError = runtime.lastError !== previousError;
    const joined = runtime.loadCatalogIndex();
    const requestsAfterFailure = pending.length;
    pending[3].resolve(index('replacement'));
    await Promise.all([replacement, joined]);

    runtime.clearCatalogCache();
    const late = runtime.loadCatalogIndex();
    runtime.clearCatalogCache();
    const latest = runtime.loadCatalogIndex();
    pending[5].resolve(index('latest'));
    await latest;
    pending[4].resolve(index('late'));
    await late;
    await runtime.loadCatalogIndex();
    const finalMarker = runtime.catalogIndex.names.marker;
    runtime.clearCatalogCache();
    const failure = runtime.loadCatalogIndex();
    pending[6].reject('current catalog failure');
    await failure;
    const currentError = runtime.lastError;
    const retry = runtime.loadCatalogIndex();
    pending[7].resolve(index('recovered'));
    await retry;
    return {requestsAfterClear, stalePublished, requestsAfterOld, first, staleError,
      requestsAfterFailure, finalMarker, currentError, recovered: runtime.catalogIndex.names.marker,
      totalRequests: pending.length};
  });
  assert.deepEqual(result, {requestsAfterClear: 2, stalePublished: false, requestsAfterOld: 2,
    first: {marker: 'fresh', icons: 1}, staleError: false, requestsAfterFailure: 4, finalMarker: 'latest', currentError: 'current catalog failure', recovered: 'recovered', totalRequests: 8});
  assert.deepEqual(errors, []);
  console.log('Catalog cache passed: invalidation starts fresh requests, stale success/error isolation and pending request ownership.');
}).catch(error => {console.error(error); process.exitCode = 1;});
