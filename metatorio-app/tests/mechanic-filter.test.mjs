import test from 'node:test';
import assert from 'node:assert/strict';
import { flowObjectKey, matchesFlow, matchesMechanic, MechanicFlowCache } from '../src/lib/mechanic-filter.ts';

const item = (id, quality = 'normal') => ({Item: {id, quality}});
const fluid = (name, min, max = min) => ({Fluid: {name, temperature: [min, max]}});
const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => {resolve=a;reject=b}); return {promise,resolve,reject}; };

for (const quality of ['normal', 'rare', 'legendary', 'mod-quality']) {
  test(`item wildcard includes ${quality}; exact quality remains restrictive`, () => {
    assert.ok(matchesFlow(item('iron',quality), {flow:item('iron')}));
    assert.equal(matchesFlow(item('iron',quality), {flow:item('iron'),quality:'rare'}), quality==='rare');
    assert.ok(!matchesFlow(item('copper',quality), {flow:item('iron')}));
    assert.ok(!matchesFlow({Entity:{id:'iron',quality}}, {flow:item('iron')}));
    assert.ok(!matchesFlow({VirtualScience:'iron'}, {flow:item('iron')}));
  });
}
test('fluid identity ignores temperatures unless explicitly refined', () => {
  for (const range of [[15,15],[165,165],[165,500],[-273,100000]]) {
    assert.ok(matchesFlow(fluid('steam',...range), {flow:fluid('steam',0)}));
    assert.equal(matchesFlow(fluid('steam',...range), {flow:fluid('steam',0),temperature:[165,500]}), range[0]===165 && range[1]===500);
  }
  assert.ok(!matchesFlow(fluid('water',165), {flow:fluid('steam',165)}));
});
test('heterogeneous flows retain their identities and fuel constraints', () => {
  const flows = ['Electricity','Heat','RocketSlotCapacity','RocketWeightCapacity',
    {VirtualScience:'science'}, {Pollution:{name:'pollution'}}, {Custom:{name:'pollution'}},
    {FluidHeat:{filter:'steam'}}, {FluidFuel:{filter:'steam'}},
    {ItemFuel:{category:['chemical'],has_burnt_result:false}},
    {ItemFuelSupply:{category:['chemical'],has_burnt_result:false}},
  ];
  assert.equal(new Set(flows.map(flowObjectKey)).size, flows.length);
  for (const flow of flows) assert.ok(matchesFlow(flow,{flow}));
  assert.equal(flowObjectKey({ItemFuel:{category:['a','b']}}), flowObjectKey({ItemFuel:{category:['b','a'],has_burnt_result:false}}));
  assert.notEqual(flowObjectKey({ItemFuel:{category:['a']}}), flowObjectKey({ItemFuel:{category:['a'],has_burnt_result:true}}));
});
test('input/output are signed nonzero net flows and combined with AND', () => {
  const flows = [{flow:item('ore'),amount:-2}, {flow:item('iron','rare'),amount:1}, {flow:'Heat',amount:0}];
  const input = {flow:item('ore')}, output = {flow:item('iron')};
  assert.ok(matchesMechanic(flows,input,output));
  assert.ok(matchesMechanic(flows,null,output));
  assert.ok(matchesMechanic(flows,input,null));
  assert.ok(!matchesMechanic(flows,output,null));
  assert.ok(!matchesMechanic(flows,null,input));
  assert.ok(!matchesMechanic(flows,input,{...output,quality:'normal'}));
  assert.ok(!matchesMechanic(flows,null,{flow:'Heat'}));
  assert.ok(matchesMechanic([],null,null));
  assert.ok(!matchesMechanic([],input,null));
});
test('card and filter requests deduplicate, successful empty flows stay cached', async () => {
  const cache = new MechanicFlowCache(), result = deferred(); let calls=0;
  const load = () => { calls++; return result.promise; };
  const first = cache.get('p:f:1','config',load), second = cache.get('p:f:1','config',load);
  assert.equal(first,second);
  result.resolve([]);
  assert.deepEqual(await first,[]);
  assert.deepEqual(await cache.get('p:f:1','config',load),[]);
  assert.equal(calls,1);
});
test('failures retry; stale rejection cannot evict newer config/environment data', async () => {
  const cache = new MechanicFlowCache(), old = deferred();
  const failed = cache.get('mechanic','old-environment',()=>old.promise);
  const fresh = [{flow:'Heat',amount:-5}];
  assert.equal(await cache.get('mechanic','new-environment',async()=>fresh),fresh);
  old.reject(Error('obsolete'));
  await assert.rejects(failed,/obsolete/);
  assert.equal(await cache.get('mechanic','new-environment',()=>{throw Error('must be cached')}),fresh);
  await assert.rejects(cache.get('other','config',async()=>{throw Error('network')}),/network/);
  assert.equal(await cache.get('other','config',async()=>fresh),fresh);
});
test('clear invalidates pending data; keys isolate projects, factories and contexts', async () => {
  const cache = new MechanicFlowCache(), old = deferred();
  const stale = cache.get('ctx:p:f:1','config',()=>old.promise);
  cache.clear();
  const fresh = [{flow:'Electricity',amount:10}];
  await cache.get('ctx:p:f:1','config',async()=>fresh);
  old.resolve([]); await stale;
  assert.equal(await cache.get('ctx:p:f:1','config',()=>{throw Error('cached')}),fresh);
  assert.deepEqual(await cache.get('ctx:p:other-factory:1','config',async()=>[]),[]);
});
