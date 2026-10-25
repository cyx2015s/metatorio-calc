import test from 'node:test';
import assert from 'node:assert/strict';
import {buildNetwork} from '../src/lib/mechanics/network.ts';
import {layoutLocations} from '../src/lib/mechanics/location-layout.ts';
import {locatedEntries} from '../src/lib/mechanics/located.ts';

const planet=name=>({kind:'planet',name}),space={kind:'space'},nauvis=planet('nauvis'),fulgora=planet('fulgora');
const item={Item:{id:'iron-plate',quality:'rare'}};
const transport=(id,type,location)=>({id,enabled:true,mechanic:{type,location,item:item.Item}});
const local=(id,location)=>({id,enabled:true,mechanic:{type:'local',location,mechanic:{type:'recipe',recipe:{id:'iron',quality:'rare'},machine:{id:'furnace',quality:'normal'}}}});
const flow=(location,amount)=>({flow:item,location,amount});
const entries=[transport(1,'rocket-export',nauvis),transport(2,'orbital-drop',fulgora),local(3,nauvis),local(4,fulgora)];
const index=new Map([[1,[flow(nauvis,-1),{location:nauvis,flow:'RocketWeightCapacity',amount:-50000},flow(space,1)]],
  [2,[flow(space,-1),flow(fulgora,1)]],[3,[flow(nauvis,4)]],[4,[flow(fulgora,-4)]]]);
function build(extra={}) {
  return buildNetwork({entries:locatedEntries({mechanics:entries}),index,visibleIds:null,isolateLocations:true,
    solutions:new Map([[1,{amount:3}],[2,{amount:2}],[3,{amount:5}],[4,{amount:7}]]),solved:true,
    targets:[],externalInputs:[],implicitInputs:[],localize:(_k,n)=>n,timeScale:'seconds',mergeRecipes:false,
    collapse:false,showUtilities:true,onlyEnabled:false,...extra});
}
const assertLocal=model=>{
  const nodes=new Map(model.nodes.map(n=>[n.id,n]));
  for(const e of model.edges)assert.deepEqual(nodes.get(e.source).location,nodes.get(e.target).location);
};
for(const collapse of [false,true])test(`clustering hides only cross-partition legs with collapse=${collapse}`,()=>{
  const shown=build({collapse});assertLocal(shown);
  assert.equal(shown.nodes.filter(n=>n.kind==='mechanic').length,4);
  assert.ok(!shown.nodes.some(n=>n.location.kind==='space'));
  assert.ok(shown.edges.some(e=>e.target==='m:1'&&e.flow==='RocketWeightCapacity'));
  assert.ok(shown.edges.some(e=>e.target==='m:1'&&typeof e.flow==='object'&&'Item' in e.flow));
  assert.ok(shown.edges.some(e=>e.source==='m:2'&&typeof e.flow==='object'&&'Item' in e.flow));
  if(collapse) {
    assert.match(shown.edges.find(e=>e.source==='m:3'&&e.target==='m:1').label,/产 20\/s → 耗 3\/s/);
    assert.match(shown.edges.find(e=>e.source==='m:2'&&e.target==='m:4').label,/产 2\/s → 耗 28\/s/);
  }
});
test('ordinary network still includes the full transport chain and leaves cached inputs intact',()=>{
  const before=JSON.stringify([...index]),shown=build({isolateLocations:false,collapse:true});
  assert.ok(shown.edges.some(e=>e.source==='m:1'&&e.target==='m:2'));
  assert.equal(JSON.stringify([...index]),before);
  assert.ok(build({isolateLocations:false}).nodes.some(n=>n.location.kind==='space'));
});
test('space targets and actual local space production/consumption are preserved',()=>{
  const shown=build({entries:locatedEntries({mechanics:[...entries,local(5,space),local(6,space)]}),
    index:new Map([...index,[5,[flow(space,1)]],[6,[flow(space,-1)]]]),locatedTargets:[{location:space,flow:item}]});
  assertLocal(shown);
  assert.ok(shown.nodes.some(n=>n.target&&n.location.kind==='space'));
  assert.equal(shown.edges.filter(e=>e.source==='m:5'||e.target==='m:6').length,2);
  const targetOnly=build({locatedTargets:[{location:space,flow:item}]});
  assert.ok(targetOnly.nodes.some(n=>n.target&&n.location.kind==='space'));
  assertLocal(targetOnly);
});
test('filtering and disabled mechanisms never reintroduce cross-location connections',()=>{
  const shown=build({visibleIds:[1],solved:false});assertLocal(shown);
  assert.equal(shown.nodes.filter(n=>n.kind==='mechanic').length,1);
  assert.match(shown.edges.find(e=>typeof e.flow==='object').label,/单位 1\/s/);
  const empty=build({visibleIds:[]});assert.deepEqual(empty.nodes,[]);assert.deepEqual(empty.edges,[]);
  const disabled=build({entries:locatedEntries({mechanics:entries}).map(e=>({...e,enabled:e.id!==1})),onlyEnabled:true});
  assert.ok(!disabled.nodes.some(n=>n.id==='m:1'));assertLocal(disabled);
});
test('location layout no longer creates empty transport hubs or cross-cluster links',async()=>{
  const calls=[],laid=await layoutLocations(build({collapse:true}),async graph=>{
    calls.push(graph);return {...graph,width:200,height:120,children:graph.children.map((n,i)=>({...n,x:i*300,y:0}))};
  });
  assert.equal(laid.groups.length,2);assert.ok(laid.groups.every(g=>g.nodes.length));
  assert.deepEqual(calls.at(-1).edges,[]);
});
