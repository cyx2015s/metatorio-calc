const path = require('node:path');
const { spawn } = require('node:child_process');

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(check, attempts = 100) {
  for (let i = 0; i < attempts; i++) {
    if (await check()) return;
    await pause(50);
  }
  throw Error('Timed out waiting for UI state');
}

/** Own only server/browser lifetime; each regression owns its pages and assertions. */
async function withBrowser({ port, externalUrl, preview = false, attempts = 600 }, run) {
  const root = path.resolve(__dirname, '..');
  const url = externalUrl || `http://127.0.0.1:${port}`;
  const server = externalUrl ? null : spawn(process.execPath, [
    path.join(root, 'node_modules/vite/bin/vite.js'), ...(preview ? ['preview'] : []),
    '--host', '127.0.0.1', '--port', String(port), '--strictPort',
  ], { cwd: root, windowsHide: true, stdio: 'ignore' });
  let browser;
  try {
    await waitFor(async () => {
      try { return (await fetch(url)).ok; } catch { return false; }
    }, attempts);
    const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
    browser = await chromium.launch({
      headless: true,
      ...(process.env.BROWSER_EXECUTABLE ? { executablePath: process.env.BROWSER_EXECUTABLE } : {}),
    });
    return await run(browser, url);
  } finally {
    try { await browser?.close(); } finally { server?.kill(); }
  }
}

module.exports = { withBrowser, waitFor, pause };
