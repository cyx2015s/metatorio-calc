import test from 'node:test';
import assert from 'node:assert/strict';
import {buildNetwork} from '../src/lib/mechanics/network.ts';
import {clusterTransportNetwork} from '../src/lib/mechanics/location-network.ts';
import {layoutLocations} from '../src/lib/mechanics/location-layout.ts';
import {locatedEntries} from '../src/lib/mechanics/located.ts';

const planet=name=>({kind:'planet',name}),space={kind:'space'},nauvis=planet('nauvis'),fulgora=planet('fulgora');
const item={Item:{id:'iron-plate',quality:'rare'}};
const localize=(_kind,name)=>name==='iron-plate'?'铁板':name==='rare'?'稀有':name;
const transport=(id,type,location)=>({id,enabled:true,mechanic:{type,location,item:item.Item}});
const local=(id,location)=>({id,enabled:true,mechanic:{type:'local',location,mechanic:{type:'recipe',recipe:{id:'iron',quality:'rare'},machine:{id:'furnace',quality:'normal'}}}});
const flow=(location,amount)=>({flow:item,location,amount});
function build({collapse=false,entries=[transport(1,'rocket-export',nauvis),transport(2,'orbital-drop',fulgora)],index=new Map([[1,[flow(nauvis,-1),{location:nauvis,flow:'RocketWeightCapacity',amount:-50000},flow(space,1)]],[2,[flow(space,-1),flow(fulgora,1)]]]),...extra}={}) {
  return buildNetwork({entries:locatedEntries({mechanics:entries}),index,visibleIds:null,
    solutions:new Map([[1,{amount:3}],[2,{amount:2}],[3,{amount:5}]]),solved:true,targets:[],externalInputs:[],implicitInputs:[],
    localize,timeScale:'seconds',mergeRecipes:false,collapse,showUtilities:true,onlyEnabled:false,...extra});
}
const project=raw=>clusterTransportNetwork(raw,localize,'seconds');

for(const collapse of [false,true]) test(`transport terminates at space boundaries with collapse=${collapse}`,()=>{
  const raw=build({collapse}),before=JSON.stringify(raw),shown=project(raw);
  const edges=shown.edges.filter(e=>e.kind==='transport');assert.equal(edges.length,2);
  assert.deepEqual(edges.map(e=>[e.source,e.target]),[['m:1','location:space'],['location:space','m:2']]);
  assert.deepEqual(edges.map(e=>e.label),['铁板 · 稀有\n3/s','铁板 · 稀有\n2/s']);
  assert.deepEqual(edges.map(e=>e.contributions.map(c=>c.mechanic)),[[1],[2]]);
  assert.ok(shown.edges.some(e=>e.target==='m:1'&&e.kind==='flow'));
  assert.ok(shown.edges.some(e=>e.source==='m:2'&&e.kind==='flow'));
  assert.ok(shown.edges.some(e=>e.target==='m:1'&&e.kind==='flow'&&e.flow==='RocketWeightCapacity'));
  assert.ok(!shown.nodes.some(n=>n.kind==='flow'&&n.location.kind==='space'));
  assert.ok(shown.locations.some(l=>l.kind==='space'));
  assert.equal(JSON.stringify(raw),before,'flat graph and document semantics remain unchanged');
});

test('restores a collapsed space recipe leg without claiming that its amount equals transport amount',()=>{
  for(const outgoing of [true,false]) {
    const entries=[transport(1,outgoing?'rocket-export':'orbital-drop',nauvis),local(3,space)];
    const raw=build({collapse:true,entries,index:new Map([[1,[flow(space,outgoing?1:-1)]],[3,[flow(space,outgoing?-4:4)]]])});
    const shown=project(raw),restored=shown.nodes.find(n=>n.kind==='flow');
    assert.ok(restored);assert.deepEqual(restored.location,space);
    assert.match(restored.search,/iron-plate/);assert.match(restored.search,/太空/);
    const localEdge=shown.edges.find(e=>e.kind==='flow');
    assert.equal(localEdge[outgoing?'source':'target'],restored.id);
    assert.deepEqual(localEdge.contributions.map(c=>c.mechanic),[3]);
    assert.equal(localEdge.label,'20/s');
    assert.equal(shown.edges.find(e=>e.kind==='transport')[outgoing?'target':'source'],'location:space');
  }
});

test('keeps targets and real space producers/consumers; filters never resurrect hidden mechanics',()=>{
  const raw=build({locatedTargets:[{location:space,flow:item}],visibleIds:[1]});
  const shown=project(raw);
  assert.ok(shown.nodes.some(n=>n.target&&n.location.kind==='space'));
  assert.ok(!shown.nodes.some(n=>n.id==='m:2'));
  assert.equal(shown.edges.filter(e=>e.kind==='transport').length,1);
  const empty=project(build({visibleIds:[]}));assert.deepEqual(empty.nodes,[]);assert.deepEqual(empty.edges,[]);
});

test('local item launch is not treated as inter-location transport; unsolved labels retain unit basis',()=>{
  const raw=build({solved:false});const shown=project(raw);
  assert.match(shown.edges.find(e=>e.kind==='transport').label,/单位 1\/s/);
  const localRaw=build({entries:[local(3,nauvis)],index:new Map([[3,[flow(nauvis,1)]]])});
  assert.deepEqual(project(localRaw).edges,localRaw.edges);
});

test('empty remote domain still gets a draggable box and participates in location layout',async()=>{
  const shown=project(build({collapse:true})),calls=[];
  const laid=await layoutLocations(shown,async graph=>{
    calls.push(graph);return {...graph,width:200,height:120,children:graph.children.map((n,i)=>({...n,x:i*300,y:0}))};
  });
  assert.ok(laid.groups.some(g=>g.id==='location:space'&&g.nodes.length===0));
  assert.ok(laid.positions.has('location:space'));
  assert.deepEqual(calls.at(-1).edges.map(e=>[e.sources[0],e.targets[0]]),[
    ['location:planet:nauvis','location:space'],['location:space','location:planet:fulgora']]);
});
