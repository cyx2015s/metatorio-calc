import test from 'node:test';
import assert from 'node:assert/strict';
import { buildNetwork, networkFlowKey, networkCompatible, networkRate, networkIconQualities } from '../src/lib/mechanics/network.ts';

const item = (id, quality = 'normal') => ({Item: {id, quality}});
const fluid = (name, ...temperature) => ({Fluid: {name, temperature}});
const entry = (id, type = 'recipe', extra = {}) => ({id, enabled: true,
  mechanic: {type, recipe: {id: 'smelt', quality: 'normal'}, machine: {id: 'furnace', quality: 'normal'}, ...extra}});
const base = overrides => ({entries: [], index: new Map(), visibleIds: null, solutions: new Map(), solved: false,
  targets: [], externalInputs: [], implicitInputs: [], localize: (_kind, name) => name, timeScale: 'seconds',
  mergeRecipes: false, collapse: false, showUtilities: true, onlyEnabled: false, ...overrides});

test('icon quality follows the depicted prototype and preserves mixed groups', () => {
  const node = entries => ({entries});
  assert.deepEqual(networkIconQualities({flow: item('iron', 'rare'), entries: []}), ['rare']);
  assert.deepEqual(networkIconQualities({flow: fluid('water', 15, 15), entries: []}), []);
  assert.deepEqual(networkIconQualities(node([entry(1), entry(2, 'recipe', {recipe: {id: 'smelt', quality: 'rare'}}), entry(3)])), ['normal', 'rare']);
  assert.deepEqual(networkIconQualities(node([entry(1, 'mining', {resource: 'ore', machine: {id: 'drill', quality: 'legendary'}})])), []);
  assert.deepEqual(networkIconQualities(node([entry(1, 'tile-dispose', {tile: 'lava', item: {id: 'iron', quality: 'rare'}})])), []);
  for (const [type, field] of [['solar', 'solar_panel'], ['generator', 'generator'], ['boiler', 'boiler'], ['reactor', 'reactor'], ['plant', 'seed'], ['spoil', 'item'], ['virtual-science', 'item']]) {
    assert.deepEqual(networkIconQualities(node([entry(1, type, {[field]: {id: 'prototype', quality: 'legendary'}})])), ['legendary']);
  }
});

test('exact flow nodes preserve quality, temperature, entity and virtual science identities', () => {
  const flows = [item('iron'), item('iron', 'rare'), fluid('water', 15, 15), fluid('water', 15, 100),
    {Entity: {id: 'iron', quality: 'normal'}}, {VirtualScience: 'iron'},
    {ItemFuel: {category: ['chemical'], has_burnt_result: false}},
    {ItemFuelSupply: {category: ['chemical'], has_burnt_result: false}}];
  const graph = buildNetwork(base({entries: [entry(1)], index: new Map([[1, flows.map(flow => ({flow, amount: 1}))]])}));
  assert.equal(graph.nodes.filter(n => n.kind === 'flow').length, flows.length);
  assert.equal(new Set(flows.map(networkFlowKey)).size, flows.length);
  assert.equal(networkFlowKey({ItemFuel: {category: ['a', 'b']}}), networkFlowKey({ItemFuel: {category: ['b', 'a']}}));
});

test('temperature compatibility is directional containment, never overlap union or transitive heating', () => {
  assert.ok(networkCompatible(fluid('steam', 165, 165), fluid('steam', 100, 500)));
  assert.ok(!networkCompatible(fluid('steam', 100, 500), fluid('steam', 165, 165)));
  assert.ok(!networkCompatible(fluid('steam', 100, 200), fluid('steam', 150, 500)));
  assert.ok(!networkCompatible(fluid('steam', 100, 100), fluid('steam', 200, 200)));
  assert.ok(!networkCompatible(fluid('steam', 100, 100), fluid('water', 0, 500)));
});

test('fuel compatibility keeps supply, demand, category overlap and burnt-slot restrictions', () => {
  const supply = (category, burnt = false) => ({ItemFuelSupply: {category, has_burnt_result: burnt}});
  const demand = (category, burnt = false) => ({ItemFuel: {category, has_burnt_result: burnt}});
  assert.ok(networkCompatible(supply(['a', 'b']), demand(['b', 'c'])));
  assert.ok(!networkCompatible(demand(['a']), supply(['a'])));
  assert.ok(!networkCompatible(supply(['a'], true), demand(['a'])));
  assert.ok(networkCompatible(supply(['a']), demand(['a'], true)));
  assert.ok(!networkCompatible(supply([]), demand(['a'])));
});

test('solved edges use actual per-mechanism amounts, including explicit zero and disabled rows', () => {
  const entries = [entry(1), entry(2), {...entry(3), enabled: false}];
  const index = new Map(entries.map(e => [e.id, [{flow: item('iron'), amount: 2}]]));
  const graph = buildNetwork(base({entries, index, solved: true, mergeRecipes: true,
    solutions: new Map([[1, {amount: 3, cost: 0}], [2, {amount: 0, cost: 0}], [3, {amount: 50, cost: 0}]])}));
  const edge = graph.edges.find(e => e.kind === 'flow');
  assert.equal(edge.label, '6/s');
  assert.deepEqual(edge.contributions.map(c => c.amount), [6, 0, 0]);
  assert.equal(graph.nodes.find(n => n.kind === 'mechanic').entries.length, 3);
});

test('unsolved merged variants list unit rates separately instead of summing alternatives', () => {
  const graph = buildNetwork(base({entries: [entry(1), entry(2)], mergeRecipes: true,
    index: new Map([[1, [{flow: item('iron'), amount: 2}]], [2, [{flow: item('iron'), amount: 4}]]])}));
  assert.equal(graph.edges[0].label, '单位 2/s / 4/s');
});

test('filter first, aggregate second; zero flow is ignored and disabled visibility is explicit', () => {
  const entries = [entry(1), entry(2), {...entry(3), enabled: false}];
  const index = new Map(entries.map(e => [e.id, [{flow: item('iron', e.id === 2 ? 'rare' : 'normal'), amount: 1}, {flow: 'Heat', amount: 0}]]));
  const graph = buildNetwork(base({entries, index, visibleIds: [2, 3], mergeRecipes: true, onlyEnabled: true}));
  assert.deepEqual(graph.nodes.find(n => n.kind === 'mechanic').entries.map(e => e.id), [2]);
  assert.ok(!graph.nodes.some(n => n.flow === 'Heat'));
});

test('collapse preserves quantities on both ends, target nodes and distinct parallel flows', () => {
  const graph = buildNetwork(base({entries: [entry(1), entry(2, 'mining')], collapse: true,
    targets: [item('goal')], index: new Map([
      [1, [{flow: item('iron'), amount: 2}, {flow: item('copper'), amount: 4}]],
      [2, [{flow: item('iron'), amount: -3}, {flow: item('copper'), amount: -1}]],
    ])}));
  assert.equal(graph.edges.filter(e => e.kind === 'collapsed').length, 2);
  assert.ok(graph.edges.some(e => e.label.includes('产 单位 2/s → 耗 单位 3/s')));
  assert.ok(graph.nodes.some(n => n.target));
});

test('explicit and implicit sources retain identity; explicit overrides only its exact implicit flow', () => {
  const graph = buildNetwork(base({entries: [entry(1)], index: new Map([[1,
    [{flow: item('ore'), amount: -1}, {flow: item('ore', 'rare'), amount: -1}]]]),
    externalInputs: [{id: 7, flow: item('ore'), penalty: 5}], implicitInputs: [item('ore'), item('ore', 'rare')]}));
  assert.equal(graph.nodes.filter(n => n.kind === 'external').length, 2);
  assert.equal(graph.nodes.filter(n => n.implicit).length, 1);
  assert.ok(graph.edges.filter(e => e.kind === 'external').every(e => e.label === ''));
});

test('utility visibility hides utility sources and links, without hiding mechanism nodes', () => {
  const graph = buildNetwork(base({entries: [entry(1)], showUtilities: false,
    index: new Map([[1, [{flow: 'Electricity', amount: -100}, {flow: item('iron'), amount: 1}]]]), implicitInputs: ['Electricity']}));
  assert.equal(graph.nodes.length, 2);
  assert.ok(graph.nodes.every(n => !n.utility));
  assert.equal(networkRate('Electricity', 24000, 'minutes'), '24 kW');
  assert.equal(networkRate(item('iron'), 2, 'minutes'), '120/min');
});

test('power and heat goals remain visible main-chain nodes when optional utilities are hidden', () => {
  for (const goal of ['Electricity', 'Heat']) {
    const graph = buildNetwork(base({entries: [entry(1)], showUtilities: false, targets: [goal],
      index: new Map([[1, [{flow: goal, amount: 1000}]]])}));
    assert.ok(graph.nodes.some(node => node.flow === goal && node.target && !node.utility));
    assert.equal(graph.edges.length, 1);
    assert.equal(graph.edges[0].utility, false);
  }
});

test('1000 mechanisms remain sparse and retain their original identities', () => {
  const entries = Array.from({length: 1000}, (_, i) => entry(i + 1, 'mining'));
  const index = new Map(entries.map(e => [e.id, [{flow: item('ore'), amount: 1}]]));
  const graph = buildNetwork(base({entries, index, mergeRecipes: true}));
  assert.equal(graph.nodes.length, 1001);
  assert.equal(graph.edges.length, 1000);
  const distinct = buildNetwork(base({entries, index: new Map(entries.map(e =>
    [e.id, [{flow: item(`ore-${e.id}`), amount: 1}]])), mergeRecipes: true}));
  assert.equal(distinct.nodes.length, 2000);
  assert.equal(distinct.edges.length, 1000);
});
