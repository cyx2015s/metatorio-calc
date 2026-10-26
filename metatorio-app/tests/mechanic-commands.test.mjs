import test from 'node:test';
import assert from 'node:assert/strict';
import { MechanicCommands } from '../src/lib/runtime/mechanic-commands.ts';

function fixture() {
  const first = {mechanics: [
    {id: 1, mechanic: {type: 'recipe'}},
    {id: 2, mechanic: {type: 'mining'}},
    {id: 3, mechanic: {type: 'tile-extract'}},
  ]};
  const second = {mechanics: [{id: 1, mechanic: {type: 'boiler'}}]};
  const factories = new Map([['10:20', first], ['11:20', second]]);
  const messages = [];
  const host = {
    getFactory: ({project, factory}) => factories.get(`${project}:${factory}`) ?? null,
    async send(message) { messages.push(message); },
  };
  const scope = {project: 10, factory: 20};
  return {first, factories, messages, host, scope, commands: new MechanicCommands(host, scope)};
}

test('captured scope and mechanism kind do not follow later selection or caller mutation', async () => {
  const {commands, scope, messages} = fixture();
  scope.project = 11;
  await commands.setMachine(1, 'assembler', 'rare');
  assert.equal(messages[0].action.project, 10);
  assert.equal(messages[0].action.factory, 20);
  assert.deepEqual(messages[0].action.action.mechanic.action,
    {recipe: {'set-machine': {machine: {id: 'assembler', quality: 'rare'}}}});
  assert.ok(Object.isFrozen(commands.scope));
});

test('shared machine edit retains heterogeneous recipe, mining and tile extraction actions', async () => {
  const {commands, messages} = fixture();
  for (const id of [1, 2, 3]) await commands.setMachine(id, 'machine', 'legendary');
  assert.deepEqual(messages.map(message => Object.keys(message.action.action.mechanic.action)[0]),
    ['recipe', 'mining', 'tile-extract']);
});

test('a multi-step reorder reads fresh snapshots and keeps its original factory over awaits', async () => {
  const {commands, host, first, messages} = fixture();
  host.send = async message => {
    messages.push(message);
    const {mechanic, position} = message.action.action['mechanic-list'].reorder;
    first.mechanics = structuredClone(first.mechanics);
    const index = first.mechanics.findIndex(entry => entry.id === mechanic);
    first.mechanics.splice(position, 0, ...first.mechanics.splice(index, 1));
    await Promise.resolve();
  };
  await commands.reorderMechanics([3, 2, 1]);
  assert.deepEqual(first.mechanics.map(entry => entry.id), [3, 2, 1]);
  assert.equal(messages.length, 2);
  assert.ok(messages.every(message => message.action.project === 10 && message.action.factory === 20));
});

test('deleted factory cannot redirect an old editor callback to another factory', async () => {
  const {commands, factories, messages} = fixture();
  factories.delete('10:20');
  await assert.rejects(commands.cloneMechanic(1), /目标工厂已不存在/);
  assert.equal(messages.length, 0);
});

test('failed dispatch aborts reorder instead of continuing with partial assumptions', async () => {
  const {commands, host, messages} = fixture();
  host.send = async message => { messages.push(message); throw new Error('dispatch rejected'); };
  await assert.rejects(commands.reorderMechanics([3, 2, 1]), /dispatch rejected/);
  assert.equal(messages.length, 1);
});

test('module edits use the current mechanism kind within the captured factory', async () => {
  const {commands, first, messages} = fixture();
  first.mechanics[0].mechanic.type = 'mining';
  await commands.setModuleSlot(1, 0, 'speed-module-3', 'legendary');
  assert.deepEqual(messages[0].action.action.mechanic.action,
    {mining: {module: {'set-module-slot': {slot: 0, module: {id: 'speed-module-3', quality: 'legendary'}}}}});
  await assert.rejects(commands.moduleMessage(3, 'clear-modules'), /不支持模块配置/);
  assert.equal(messages.length, 1);
});

test('fusion edits use their own action tags and support inherited neighbours',async()=>{
 const {commands,first,messages}=fixture();
 first.mechanics.push({id:4,mechanic:{type:'fusion-reactor'}},{id:5,mechanic:{type:'fusion-generator'}});
 await commands.setReactor(4,'fusion-reactor','rare');
 await commands.setNeighbours(4,null);
 await commands.setFuel(4,{kind:'item',item:{id:'fusion-power-cell',quality:'normal'}});
 await commands.setGenerator(5,'fusion-generator','legendary');
 await commands.setMechanicTemperature(5,4000000);
 assert.deepEqual(messages.map(m=>Object.keys(m.action.action.mechanic.action)[0]),['fusion-reactor','fusion-reactor','fusion-reactor','fusion-generator','fusion-generator']);
 assert.equal(messages[1].action.action.mechanic.action['fusion-reactor']['set-neighbours'].neighbours,null);
 assert.equal(messages[4].action.action.mechanic.action['fusion-generator']['set-temperature'].temperature,4000000);
});
