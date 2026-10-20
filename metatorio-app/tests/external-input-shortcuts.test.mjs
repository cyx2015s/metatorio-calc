import test from 'node:test';
import assert from 'node:assert/strict';
import { createExternalInputShortcuts, DISABLED_RESOURCE_PENALTY } from '../src/lib/external-input-shortcuts.ts';

function harness(inputs = []) {
  const factory = {external_inputs: structuredClone(inputs)}, calls = [];
  const host = {
    getFactory: scope => scope.project === 1 && scope.factory === 2 ? factory : null,
    async send(message) {
      calls.push(message);
      const action = message.action.action['external-input'];
      if (action.add) factory.external_inputs.push({...action.add.input, id: factory.external_inputs.length + 10});
      if (action['set-penalty']) factory.external_inputs.find(input => input.id === action['set-penalty'].input).penalty = action['set-penalty'].penalty;
    },
  };
  return {factory, calls, host, commands: createExternalInputShortcuts(host, {project: 1, factory: 2})};
}

test('free energy adds exact zero-cost power and heat once', async () => {
  const h = harness();
  await h.commands.addFreeEnergy();
  await h.commands.addFreeEnergy();
  assert.deepEqual(h.factory.external_inputs.map(({flow, penalty}) => ({flow, penalty})), [{flow: 'Electricity', penalty: 0}, {flow: 'Heat', penalty: 0}]);
  assert.equal(h.calls.length, 2);
});

test('free energy reuses existing entries and updates their costs without duplicates', async () => {
  const h = harness([{id: 3, flow: 'Electricity', penalty: 7}, {id: 4, flow: 'Heat', penalty: 0}]);
  await h.commands.addFreeEnergy();
  assert.deepEqual(h.calls[0].action.action, {'external-input': {'set-penalty': {input: 3, penalty: 0}}});
  assert.equal(h.calls.length, 1);
  assert.equal(h.factory.external_inputs.length, 2);
});

test('resource suppression uses a finite internal cost and exact quality/temperature identities', async () => {
  const h = harness([{id: 3, flow: {Item: {id: 'ice', quality: 'rare'}}, penalty: 0}]);
  const flow = {Item: {id: 'ice', quality: 'normal'}};
  await h.commands.setCost(flow, DISABLED_RESOURCE_PENALTY);
  await h.commands.setCost(flow, DISABLED_RESOURCE_PENALTY);
  assert.equal(h.calls.length, 1);
  assert.deepEqual(h.calls[0].action.action['external-input'].add.input, {id: 0, flow, penalty: 1e12});
  assert.equal(h.factory.external_inputs[0].penalty, 0);
  assert.ok(Number.isFinite(DISABLED_RESOURCE_PENALTY));
  await h.commands.setCost({Fluid: {name: 'water', temperature: [15, 15]}}, DISABLED_RESOURCE_PENALTY);
  await h.commands.setCost({Fluid: {name: 'water', temperature: [15, 100]}}, DISABLED_RESOURCE_PENALTY);
  assert.equal(h.factory.external_inputs.filter(input => input.flow?.Fluid).length, 2);
});

test('multi-step shortcuts keep the original scope across asynchronous UI selection changes', async () => {
  const h = harness(), scope = {project: 1, factory: 2};
  const commands = createExternalInputShortcuts(h.host, scope);
  const send = h.host.send;
  h.host.send = async message => { await send(message); scope.factory = 999; };
  await commands.addFreeEnergy();
  assert.equal(scope.factory, 999);
  assert.ok(h.calls.every(call => call.action.project === 1 && call.action.factory === 2));
});

test('failed dispatch or removed factory stops further writes', async () => {
  const h = harness();
  h.host.send = async () => { throw Error('write failed'); };
  await assert.rejects(h.commands.addFreeEnergy(), /write failed/);
  h.host.getFactory = () => null;
  await assert.rejects(h.commands.addFreeEnergy(), /不存在/);
});
