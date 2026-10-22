// Actual Svelte page with mocked IPC: progress lifetime, scope and stale events.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { seed, waitFor } = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawn } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const port = Number(process.env.UI_TEST_PORT || 5204);
(async () => {
  const server = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], {cwd: root, windowsHide: true, stdio: 'ignore'});
  let browser;
  try {
    await waitFor(async () => { try { return (await fetch(`http://127.0.0.1:${port}`)).ok; } catch { return false; } }, 600);
    browser = await chromium.launch({headless: true, ...(process.env.BROWSER_EXECUTABLE ? {executablePath: process.env.BROWSER_EXECUTABLE} : {})});
    const page = await browser.newPage({viewport: {width: 1600, height: 1000}});
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    await seed(page, 2);
    await page.addInitScript(() => {
      const callbacks = new Map(), listeners = new Map();
      const rawCallback = window.__TAURI_INTERNALS__.transformCallback;
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.transformCallback = callback => {
        const id = rawCallback(callback); callbacks.set(id, callback); return id;
      };
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'dispatch' && args.message?.action?.action?.solve === 'auto-plan') {
          await new Promise((resolve, reject) => {
            window.__completePlan = resolve;
            window.__failPlan = () => reject({code: 'auto-plan-not-solved', message: '规划失败测试', no_provider: [], no_consumer: [], flows: []});
          });
        }
        const result = await invoke(command, args);
        if (command === 'plugin:event|listen') listeners.set(args.event, callbacks.get(args.handler));
        return result;
      };
      window.__emitPlanProgress = payload => listeners.get('auto-plan-progress')?.({payload});
    });
    await page.goto(`http://127.0.0.1:${port}`);
    await page.locator('.mech-card').first().waitFor();
    const status = page.locator('.auto-plan-progress');
    const button = page.getByRole('button', {name: '自动规划', exact: true});
    const emit = payload => page.evaluate(payload => window.__emitPlanProgress(payload), payload);
    const progress = (run_id, extra = {}) => ({project: 1, factory: 2, run_id, running: true, stage: 'solving', variables: 1_000_000, constraints: 10_000, ...extra});

    await button.click();
    await waitFor(async () => (await status.textContent()).includes('待构建'));
    await emit(progress(11, {stage: 'enumerating', variables: null, constraints: null}));
    await waitFor(async () => (await status.textContent()).includes('枚举候选'));
    await emit(progress(11));
    await waitFor(async () => (await status.textContent()).includes('1,000,000 个变量'));
    assert.ok((await status.textContent()).includes('10,000 条约束'));
    await page.waitForTimeout(250);
    assert.ok((await status.textContent()).includes('1,000,000'));
    await emit(progress(12, {factory: 3, variables: 300, constraints: 20}));
    assert.ok((await status.textContent()).includes('1,000,000'), 'another factory cannot overwrite current counts');
    if (process.env.AUTO_PLAN_PROGRESS_SCREENSHOT) await page.screenshot({path: process.env.AUTO_PLAN_PROGRESS_SCREENSHOT});
    await page.evaluate(async () => {
      const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
      const other = structuredClone(window.__layoutDocument.projects[0].factories[0]); other.id = 3;
      runtime.document.projects[0].factories.push(other);
      runtime.selectedFactoryId = 3;
    });
    await waitFor(async () => (await status.textContent()).includes('300 个变量'));
    await page.evaluate(async () => { const {runtime} = await import('/src/lib/runtime/store.svelte.ts'); runtime.selectedFactoryId = 2; });
    await waitFor(async () => (await status.textContent()).includes('1,000,000'));
    await emit(progress(11, {stage: 'verifying', variables: 37, constraints: 22}));
    await waitFor(async () => (await status.textContent()).includes('37 个变量'));
    await emit(progress(11, {running: false}));
    await page.evaluate(() => window.__completePlan());
    await button.waitFor();
    await waitFor(async () => await status.count() === 0);
    await emit(progress(11));
    assert.equal(await status.count(), 0, 'a closed run cannot restart itself');

    // MCP-originated runs appear without invoking the frontend autoPlan method.
    await emit(progress(13));
    await waitFor(async () => (await status.textContent()).includes('1,000,000'));
    await emit(progress(14, {variables: 500}));
    await emit(progress(13, {variables: 1}));
    await waitFor(async () => (await status.textContent()).includes('500 个变量'));
    await emit(progress(14, {running: false}));
    await waitFor(async () => await status.count() === 0);

    await button.click();
    await emit(progress(15));
    await emit(progress(15, {running: false}));
    await page.evaluate(() => window.__failPlan());
    await waitFor(async () => await status.count() === 0);
    await button.waitFor();
    assert.equal(await button.isEnabled(), true);
    await emit(progress(15));
    assert.equal(await status.count(), 0);
    assert.deepEqual(errors, []);
    console.log('PASS: pending dimensions, persistent counts, verification, factory scope, MCP runs, stale events, success and failure cleanup');
  } finally {
    await browser?.close();
    server.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
