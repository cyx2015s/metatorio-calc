import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import * as client from '../src/lib/runtime/client.ts';
import { compactNumber } from '../src/lib/format.ts';
import { formatStat } from '../src/lib/ui/hover-model.ts';

globalThis.window = { crypto: webcrypto };
globalThis.crypto ??= webcrypto;
afterEach(() => clearMocks());

test('generated commands preserve wire names, argument casing and UI adapters', async () => {
  const calls = [];
  mockIPC((command, args) => { calls.push([command, args]); return 'reply'; });
  const flow = { Item: { id: 'iron-plate', quality: 'normal' } };
  assert.equal(await client.prototypeDetail('item', 'iron-plate', 'ctx'), 'reply');
  await client.suggest(flow, 'ctx');
  await client.allowedModules('machine', 'assembler', null, 'ctx');
  await client.loadGameContext('factorio.exe');
  const message = { scope: 'application', action: { 'new-project': { name: 'test' } } };
  await client.dispatch(message);
  assert.deepEqual(calls, [
    ['prototype_detail', { contextId: 'ctx', kind: 'item', name: 'iron-plate' }],
    ['suggest', { contextId: 'ctx', flow }],
    ['allowed_modules', { contextId: 'ctx', machineKind: 'machine', machine: 'assembler', recipe: null }],
    ['load_game_context', { executablePath: 'factorio.exe', modDir: null }],
    ['dispatch', { message }],
  ]);
});

test('string and structured command errors still reject without result wrapping', async () => {
  for (const error of ['failure', { code: 'invalid-command', message: 'invalid' }]) {
    mockIPC(() => Promise.reject(error));
    await assert.rejects(client.getDocument(), actual => actual === error);
  }
});

test('typed event subscriptions retain payloads and support unlisten', async () => {
  mockIPC(() => {}, { shouldMockEvents: true });
  const cases = [
    ['auto-plan-progress', client.onAutoPlanProgress, { project: 1, factory: 2, universe: null, run_id: 3, running: true, stage: 'solving', variables: 10, constraints: 4 }],
    ['solve-error', client.onSolveError, { project: 1, factory: 2, revision: 3, code: 'solver', message: 'failure' }],
    ['solve-error', client.onSolveError, 'legacy string failure'],
    ['command-error', client.onCommandError, ['failure', { code: 'invalid', message: 'failure' }]],
    ['universe-solve-error', client.onUniverseSolveError, { project: 1, universe: 2, revision: 3, message: 'failure' }],
    ['document-changed', client.onDocumentChanged, 42],
    ['document-changed', client.onDocumentChanged, null],
  ];
  for (const [name, listen, payload] of cases) {
    const received = [];
    const unlisten = await listen(value => received.push(value));
    await emit(name, payload);
    assert.deepEqual(received, [payload]);
    await unlisten();
    await emit(name, payload);
    assert.equal(received.length, 1);
  }
});

test('browser command guard produces a useful rejection', async () => {
  delete window.__TAURI_INTERNALS__;
  await assert.rejects(client.getDocument(), /不在 Tauri 环境中/);
});

test('JSON null floats are displayed as unavailable instead of zero', () => {
  assert.equal(compactNumber(null), '—');
  assert.equal(compactNumber(NaN), '—');
  assert.equal(formatStat({ key: 'power', value: null, unit: 'watts' }), '—');
});
