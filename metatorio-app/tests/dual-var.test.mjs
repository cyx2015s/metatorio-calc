import test from 'node:test';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import { presentFlow, isEnergyFlow } from '../src/lib/dual-var.ts';
import { diagnosticFrom, diagnosticText } from '../src/lib/diagnostic.ts';

const labels = {'item/iron':'铁板','entity/iron':'铁实体','item/science':'科研包','fluid/steam':'蒸汽','quality/normal':'普通','quality/rare':'稀有','quality/legendary':'传说','fuel-category/chemical':'化学燃料','airborne-pollutant/pollution':'污染'};
const localize = (kind,name) => labels[`${kind}/${name}`] ?? name;
const item = {Item:{id:'iron',quality:'rare'}};

const references=JSON.parse(readFileSync(new URL('../../tests/fixtures/dual-var.json',import.meta.url),'utf8'));
for (const {flow,type,name,badge} of references) test(`presentation retains semantic identity: ${JSON.stringify(flow)}`,()=>{
  const info=presentFlow(flow,localize);
  assert.deepEqual([info.type,info.name,info.badge],[type,name,badge]);
  assert.ok(info.label); assert.ok(info.description.includes(info.kindLabel));
  assert.equal(info.wildcard,false);
});

test('patterns never inherit the representative quality or temperature',()=>{
  const wildcard=presentFlow(item,localize,{});
  assert.equal(wildcard.quality,undefined); assert.equal(wildcard.wildcard,true);
  assert.deepEqual(wildcard.facts,[{label:'匹配品质',value:'任意品质'}]);
  const exact=presentFlow(item,localize,{quality:'normal'});
  assert.equal(exact.quality,'normal'); assert.equal(exact.wildcard,false);
  assert.ok(exact.description.includes('普通'));
  const fluid={Fluid:{name:'steam',temperature:[500,500]}};
  assert.equal(presentFlow(fluid,localize,{}).badge,'任意温度');
  assert.equal(presentFlow(fluid,localize,{temperature:[165,165]}).badge,'165℃');
  assert.deepEqual(item,{Item:{id:'iron',quality:'rare'}});
});
test('burnt-result capability means different things on the supply and demand sides',()=>{
  const demand=presentFlow({ItemFuel:{category:['chemical'],has_burnt_result:true}},localize);
  const supply=presentFlow({ItemFuelSupply:{category:['chemical'],has_burnt_result:true}},localize);
  assert.ok(demand.description.includes('可接收燃尽产物'));
  assert.ok(supply.description.includes('有燃尽产物'));
  assert.equal(demand.facts[0].value,'化学燃料');
  assert.equal(presentFlow({ItemFuel:{category:[]}},localize).facts[0].value,'空类别集合');
});
test('units follow flow semantics rather than icon or object category',()=>{
  for (const flow of ['Electricity','Heat',{FluidHeat:{filter:''}},{FluidFuel:{filter:''}},{ItemFuel:{category:[]}},{ItemFuelSupply:{category:[]}}]) assert.ok(isEnergyFlow(flow));
  for (const flow of [item,{VirtualScience:'science'},{Fluid:{name:'steam',temperature:[15,15]}},'RocketWeightCapacity']) assert.ok(!isEnergyFlow(flow));
});
test('structured errors preserve objects and copy text remains searchable',()=>{
  const diagnostic=diagnosticFrom({code:'invalid-flow',message:'无法处理该流',flows:[item],no_consumer:[{Fluid:{name:'steam',temperature:[165,500]}}]});
  assert.deepEqual(diagnostic.flows,[item]);
  const text=diagnosticText(diagnostic,localize);
  assert.match(text,/铁板.*稀有/); assert.match(text,/蒸汽.*165～500℃/);
  assert.ok(text.includes(JSON.stringify(item)));
  assert.equal(diagnosticFrom(Error('plain error')).message,'plain error');
  assert.equal(diagnosticFrom('legacy string').message,'legacy string');
  assert.deepEqual(diagnosticFrom({message:'bad payload',flows:[null,{},1,{Item:null},{Fluid:{name:'x',temperature:[1]}}]}).flows,[]);
});
