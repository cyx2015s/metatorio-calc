import test from 'node:test';
import assert from 'node:assert/strict';
import {locatedEntries,universeView} from '../src/lib/mechanics/located.ts';
import {buildNetwork} from '../src/lib/mechanics/network.ts';

const planet=name=>({kind:'planet',name}), space={kind:'space'};
const item={Item:{id:'iron',quality:'normal'}};
const local=(id,location)=>({id,enabled:true,mechanic:{type:'local',location,mechanic:{type:'recipe',recipe:{id:'iron',quality:'normal'},machine:{id:'furnace',quality:'normal'}}}});
const universe={id:7,name:'宇宙',strict_source:true,strict_sink:false,mechanics:[local(1,planet('nauvis')),local(2,planet('fulgora')),{id:3,enabled:true,mechanic:{type:'rocket-export',location:planet('nauvis'),item:item.Item}}],targets:[{id:4,flow:{location:space,flow:item},amount:1}],target_expressions:[{id:5,constant:0,terms:[{id:6,flow:{location:planet('fulgora'),flow:item},coefficient:1}]}]};
const source=extra=>({entries:locatedEntries(universe),index:new Map(),visibleIds:null,solutions:new Map(),solved:false,targets:[],externalInputs:[],implicitInputs:[],localize:(_k,n)=>n,timeScale:'seconds',mergeRecipes:true,collapse:false,showUtilities:true,onlyEnabled:false,...extra});

test('located projection keeps IDs, location and transfer identity without mutating documents',()=>{
  const before=JSON.stringify(universe),view=universeView(universe);
  assert.equal(view.mechanics[2].transport,"rocket-export");assert.deepEqual(view.targets[0].location,space);
  assert.deepEqual(view.target_expressions[0].terms[0].location,planet('fulgora'));
  assert.equal(JSON.stringify(universe),before);
});

test('located external inputs preserve location and replace only matching implicit network sources',()=>{
  const flow={location:planet('nauvis'),flow:item};
  const external={id:88,flow,penalty:9};
  assert.deepEqual(universeView({...universe,external_inputs:[external]}).external_inputs,[{id:88,flow:item,location:planet('nauvis'),penalty:9}]);
  const model=buildNetwork(source({locatedTargets:[flow,{location:space,flow:item}],locatedExternalInputs:[external],locatedImplicitInputs:[flow,{location:space,flow:item}]}));
  const inputs=model.nodes.filter(n=>n.kind==='external');
  assert.equal(inputs.length,2);
  assert.equal(inputs.find(n=>n.location.kind==='planet').implicit,false);
  assert.equal(inputs.find(n=>n.location.kind==='space').implicit,true);
  assert.equal(inputs.find(n=>n.location.kind==='planet').external.penalty,9);
  assert.ok(model.nodes.filter(n=>n.location?.kind==='planet').every(n=>n.search.includes(n.location.name)));
});
test('same recipe on different planets never merges and equal items never share a flow node',()=>{
  const model=buildNetwork(source({index:new Map([[1,[{location:planet('nauvis'),flow:item,amount:1}]],[2,[{location:planet('fulgora'),flow:item,amount:-1}]]])}));
  assert.equal(model.nodes.filter(n=>n.kind==='mechanic').length,3);
  assert.equal(model.nodes.filter(n=>n.kind==='flow').length,2);
  assert.equal(new Set(model.nodes.filter(n=>n.kind==='flow').map(n=>n.id)).size,2);
});
test('transfer edges bridge its own source and space; targets and compatibility stay location scoped',()=>{
  const fluid=t=>({Fluid:{name:'water',temperature:t}});
  const index=new Map([[1,[{location:planet('nauvis'),flow:fluid([15,15]),amount:1}]],[2,[{location:planet('fulgora'),flow:fluid([15,100]),amount:-1}]],[3,[{location:planet('nauvis'),flow:item,amount:-1},{location:space,flow:item,amount:1}]]]);
  const model=buildNetwork(source({index,locatedTargets:[{location:space,flow:item}]}));
  assert.equal(model.edges.filter(e=>e.kind==='compatibility').length,0);
  assert.deepEqual(model.nodes.filter(n=>n.target).map(n=>n.location),[space]);
  assert.equal(model.edges.filter(e=>e.source==='m:3'||e.target==='m:3').length,2);
});
