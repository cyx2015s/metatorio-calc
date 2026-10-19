import test from 'node:test';
import assert from 'node:assert/strict';
import { lookupName, flowName, mechanicPrototype } from '../src/lib/display-labels.ts';

const names = new Map(Object.entries({
  'item/iron-plate': '铁板', 'entity/drill': '采矿机', 'item/module': '插件',
  'quality/rare': '稀有', 'fluid/steam': '蒸汽', 'fuel-category/chemical': '化学燃料',
  'item/chemical': '同名物品', 'airborne-pollutant/pollution': '污染',
  'space-location/nauvis': '新地星', 'planet/nauvis': '星球专用名',
}));
const localize = (kind, name) => lookupName(names, kind, name);

test('namespace lookup respects exact names, equivalent aliases and absent translations', () => {
  assert.equal(localize('mining-machine','drill'), '采矿机');
  assert.equal(localize('module','module'), '插件');
  assert.equal(localize('planet','nauvis'), '星球专用名');
  assert.equal(localize('space-location','nauvis'), '新地星');
  assert.equal(lookupName(new Map([['planet/nauvis','新地星']]),'space-location','nauvis'), '新地星');
  assert.equal(localize('fuel-category','chemical'), '化学燃料');
  assert.equal(localize('module-category','chemical'), 'chemical');
  assert.equal(localize('quality','iron-plate'), 'iron-plate');
  assert.equal(lookupName(new Map([['item/empty','']]),'item','empty'), 'empty');
  assert.equal(localize('item','unknown'), 'unknown');
});

for (const [flow, expected] of [
  ['Electricity','电力'], ['Heat','热量'], ['RocketSlotCapacity','火箭运力（槽位）'],
  [{Item:{id:'iron-plate',quality:'rare'}},'铁板'],
  [{Entity:{id:'drill',quality:'normal'}},'采矿机'],
  [{VirtualScience:'iron-plate'},'虚拟科研（铁板）'],
  [{Fluid:{name:'steam',temperature:[165,500]}},'蒸汽'],
  [{Pollution:{name:'pollution'}},'污染（污染）'],
  [{Custom:{name:'iron-plate'}},'iron-plate'],
  [{ItemFuel:{category:['chemical']}},'物品燃料'],
  [{ItemFuelSupply:{category:[]}},'物品燃料供给'],
  [{FluidFuel:{filter:'steam'}},'流体燃料'],
  [{FluidHeat:{filter:'steam'}},'流体热量'],
]) test(`human flow name: ${JSON.stringify(flow)}`, () => assert.equal(flowName(flow, localize), expected));

test('synthetic icon labels are Chinese without changing prototype IDs', () => {
  assert.equal(localize('flow','Electricity'),'电力');
  assert.equal(localize('flow','FluidHeat'),'流体热量');
  assert.equal(localize('item','Electricity'),'Electricity');
});

test('group headings include solar panels and extraction tiles; retain heterogeneous prototype kinds', () => {
  for (const [mechanic, kind, name] of [
    [{type:'solar',solar_panel:{id:'panel'}},'entity','panel'],
    [{type:'tile-extract',tile:'water'},'tile','water'],
    [{type:'mining',resource:'ore'},'resource','ore'],
    [{type:'plant',seed:{id:'seed'}},'item','seed'],
    [{type:'recipe',recipe:{id:'plate'}},'recipe','plate'],
    [{type:'virtual-science',item:{id:'science'}},'item','science'],
  ]) assert.deepEqual(mechanicPrototype(mechanic), {kind,name});
});
