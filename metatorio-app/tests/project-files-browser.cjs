const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const {seed, waitFor} = require('./mechanic-layout.cjs');
const assert = require('node:assert/strict');
const path = require('node:path');
const {spawn} = require('node:child_process');
const appRoot = path.resolve(__dirname, '..'), port = Number(process.env.UI_TEST_PORT || 5205), url = `http://127.0.0.1:${port}`;
(async () => {
  const server = spawn(process.execPath, [path.join(appRoot, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], {cwd: appRoot, windowsHide: true, stdio: 'ignore'});
  let browser;
  try {
    await waitFor(async () => {try {return (await fetch(url)).ok;} catch {return false;}}, 600);
    browser = await chromium.launch({headless: true, ...(process.env.BROWSER_EXECUTABLE ? {executablePath: process.env.BROWSER_EXECUTABLE} : {})});
    const page = await browser.newPage({viewport: {width: 1600, height: 1000}});
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
    await seed(page, 16);
    await page.addInitScript(() => {
      const doc = window.__layoutDocument, template = structuredClone(doc.projects[0]);
      const pending = ['C:/工程/启动 文件.metatorio'], opened = new Map(), callbacks = new Map();
      const transform = window.__TAURI_INTERNALS__.transformCallback;
      window.__TAURI_INTERNALS__.transformCallback = callback => {const id = transform(callback); callbacks.set(id, callback); return id;};
      const raw = window.__TAURI_INTERNALS__.invoke;
      let listener, nextId = 100;
      window.__fileCalls = [];
      window.__queueFiles = files => {pending.push(...files); callbacks.get(listener)?.({payload: null});};
      window.__TAURI_INTERNALS__.invoke = async (command, args = {}) => {
        if (command === 'plugin:event|listen' && args.event === 'project-open-requested') listener = args.handler;
        if (command === 'take_project_open_requests') return pending.splice(0);
        if (command === 'pick_project_file') return window.__pickedFile ?? null;
        if (command === 'project_save_path') return [...opened].find(([, ids]) => ids.includes(args.project))?.[0] ?? null;
        if (command === 'open_project_file') {
          const file = args.path;
          window.__fileCalls.push(file);
          if (file.includes('损坏')) throw Error(`读取工程「${file}」失败：JSON 解析失败 line 2 column 4`);
          if (file.includes('等待')) await new Promise(resolve => window.__finishFileOpen = resolve);
          if (opened.has(file)) return {path: file, projects: opened.get(file), reused: true};
          const project = structuredClone(template);
          project.id = nextId++;
          project.name = file;
          project.factories.forEach(factory => factory.id = nextId++);
          project.factories.push({...structuredClone(project.factories[0]), id: nextId++});
          project.context_id = file.includes('缺失') ? 'missing-context' : file.includes('延迟目录') ? 'late-context' : 'test';
          doc.projects.push(project);
          opened.set(file, [project.id]);
          return {path: file, projects: [project.id], reused: false};
        }
        if (command === 'catalog_index' && args.contextId === 'missing-context') throw Error('对应的游戏数据不在缓存中');
        if (command === 'catalog_index' && args.contextId === 'late-context') {
          const index = await raw(command, args);
          return new Promise(resolve => window.__finishCatalog = () => resolve({...index, context_id: 'late-context'}));
        }
        return raw(command, args);
      };
    });
    await page.goto(url);
    const state = () => page.evaluate(async () => {
      const {runtime} = await import('/src/lib/runtime/store.svelte.ts');
      return {project: runtime.selectedProjectId, factory: runtime.selectedFactoryId, name: runtime.selectedProject?.name,
        count: runtime.document?.projects.length ?? 0, opening: runtime.openingProjectPath, catalog: runtime.catalogIndex?.context_id, error: runtime.lastError};
    });
    await waitFor(async () => (await state()).name === 'C:/工程/启动 文件.metatorio' && !(await state()).opening, 600);
    const first = await state();
    assert.ok(first.factory != null, 'startup files select a factory');
    const secondFactory = await page.evaluate(async () => {const {runtime} = await import('/src/lib/runtime/store.svelte.ts'); const id = runtime.selectedProject.factories[1].id; await runtime.selectFactory(id); return id;});
    await page.evaluate(() => window.__queueFiles(['C:/工程/启动 文件.metatorio']));
    await waitFor(async () => await page.getByText(/此工程已打开，已切换到现有项目/).count() > 0);
    assert.equal((await state()).count, first.count);
    assert.equal((await state()).factory, secondFactory, 'reopening preserves the existing factory selection');
    await page.evaluate(() => window.__queueFiles(['C:/等待.metatorio']));
    await page.getByText(/正在打开工程：C:\/等待.metatorio/).waitFor();
    await waitFor(async () => await page.evaluate(() => !!window.__finishFileOpen));
    await page.evaluate(() => window.__finishFileOpen());
    await waitFor(async () => (await state()).name === 'C:/等待.metatorio' && !(await state()).opening);
    await page.evaluate(() => window.__queueFiles(['C:/损坏.metatorio', 'C:/正常.json']));
    await waitFor(async () => (await state()).name === 'C:/正常.json' && (await state()).error?.includes('损坏.metatorio'));
    assert.match((await state()).error, /line 2 column 4/);
    await page.evaluate(() => window.__queueFiles(['C:/缺失数据.metatorio']));
    await page.getByRole('button', {name: '选择游戏数据', exact: true}).waitFor();
    assert.ok((await state()).name.includes('缺失数据'), 'missing context does not lose the imported project');
    await page.getByRole('button', {name: '选择游戏数据', exact: true}).click();
    await page.getByRole('dialog', {name: '游戏数据与上下文', exact: true}).waitFor();
    await page.keyboard.press('Escape');
    await page.evaluate(() => window.__queueFiles(['C:/延迟目录.metatorio']));
    await waitFor(async () => await page.evaluate(() => !!window.__finishCatalog));
    await page.evaluate(() => window.__queueFiles(['C:/正常.json']));
    await waitFor(async () => (await state()).name === 'C:/正常.json');
    await page.evaluate(() => window.__finishCatalog());
    await page.waitForTimeout(100);
    assert.equal((await state()).catalog, 'test', 'an old context response cannot overwrite the current catalog');
    await page.evaluate(async () => {window.__pickedFile = 'C:/对话框.metatorio'; const {runtime} = await import('/src/lib/runtime/store.svelte.ts'); await runtime.openProject();});
    assert.equal((await state()).name, 'C:/对话框.metatorio', 'dialog opening uses the same typed loading path');
    await page.evaluate(async () => {window.__pickedFile = null; const {runtime} = await import('/src/lib/runtime/store.svelte.ts'); await runtime.openProject();});
    assert.equal((await state()).opening, null, 'cancelling a dialog clears busy state');
    assert.deepEqual(errors, []);
    console.log('Project file browser passed: startup inbox, OS requests, loading, same-file focus, partial failures, missing context recovery, stale catalogs and dialog reuse.');
  } finally {await browser?.close(); server.kill();}
})().catch(error => {console.error(error); process.exitCode = 1;});
