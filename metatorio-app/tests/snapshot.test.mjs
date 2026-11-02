import test from 'node:test';
import assert from 'node:assert/strict';
import { reconcileSnapshot } from '../src/lib/runtime/snapshot.ts';
import { mechanicFlowSettings } from '../src/lib/runtime/flow-settings.ts';

const fixture = () => ({ projects: [{ id: 1, settings: { time_scale: 'seconds', mining_productivity: 0 }, factories: [
  { id: 2, mechanics: [{ id: 3, mechanic: { type: 'recipe', modules: ['a', 'b'] } }], targets: [] },
  { id: 4, mechanics: [], targets: [] },
] }] });

test('local coefficient identity excludes display/backend/prices and retains physical project settings', () => {
  const before = { time_scale: 'seconds', solver: 'highs', orbital_transport_costs: { rocket_export_per_kg: 1 },
    mining_productivity: 0.5, reactor_neighbours: { reactor: 3 }, milestones: [], major_quality: 'normal' };
  const next = { ...before, time_scale: 'hours', solver: 'soplex', orbital_transport_costs: { rocket_export_per_kg: 262144 } };
  assert.deepEqual(mechanicFlowSettings(before), mechanicFlowSettings(next));
  for (const physical of [{ mining_productivity: 1 }, { reactor_neighbours: { reactor: 4 } }, { major_quality: 'rare' }, { milestones: ['changed'] }]) {
    assert.notDeepEqual(mechanicFlowSettings(before), mechanicFlowSettings({ ...next, ...physical }));
  }
});

test('project edits retain untouched factories and mechanics without modifying the previous document', () => {
  const before = fixture(), next = structuredClone(before), saved = structuredClone(before);
  next.projects[0].settings.mining_productivity = 0.5;
  const shared = reconcileSnapshot(before, next);
  assert.notEqual(shared, before);
  assert.notEqual(shared.projects[0].settings, before.projects[0].settings);
  assert.equal(shared.projects[0].factories, before.projects[0].factories);
  assert.deepEqual(before, saved);
  assert.deepEqual(shared.projects[0].settings, { time_scale: 'seconds', mining_productivity: 0.5 });
  assert.equal(reconcileSnapshot(shared, structuredClone(shared)), shared);
});

test('mechanic edits replace their path while retaining sibling data', () => {
  const before = fixture(), next = structuredClone(before);
  next.projects[0].factories[0].mechanics[0].mechanic.modules[1] = 'c';
  const expected = structuredClone(next), shared = reconcileSnapshot(before, next);
  assert.deepEqual(shared, expected);
  assert.notEqual(shared.projects[0].factories[0].mechanics, before.projects[0].factories[0].mechanics);
  assert.equal(shared.projects[0].factories[1], before.projects[0].factories[1]);
  assert.equal(shared.projects[0].settings, before.projects[0].settings);
  assert.deepEqual(before.projects[0].factories[0].mechanics[0].mechanic.modules, ['a', 'b']);
});

test('removal, ordering, null and type changes preserve the exact incoming JSON', () => {
  for (const [before, next] of [
    [{ a: 1, b: 2 }, { a: 1 }], [{ a: 1 }, { a: 1, b: null }],
    [[{ id: 1 }, { id: 2 }], [{ id: 2 }, { id: 1 }]],
    [{ a: [] }, { a: {} }], [{ a: [1, 2] }, { a: [1] }],
    [null, { a: 1 }], [{ a: 1 }, null], [false, 0],
    [JSON.parse('{"__proto__":{"x":1}}'), JSON.parse('{"__proto__":{"x":2}}')],
  ]) {
    const expected = structuredClone(next), saved = structuredClone(before);
    assert.deepEqual(reconcileSnapshot(before, next), expected);
    assert.deepEqual(before, saved);
  }
});
