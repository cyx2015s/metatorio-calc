import test from 'node:test';
import assert from 'node:assert/strict';
import ELK from 'elkjs/lib/elk.bundled.js';
import {layoutLocations} from '../src/lib/mechanics/location-layout.ts';
import {buildNetwork} from '../src/lib/mechanics/network.ts';
import {locatedEntries} from '../src/lib/mechanics/located.ts';

test('explicit transport nodes replace all hidden space delivery links',()=>{
 const planet={kind:'planet',name:'a'},space={kind:'space'},flow={Item:{id:'ore',quality:'rare'}};
 const universe={mechanics:[{id:1,enabled:true,mechanic:{type:'rocket-export',location:planet,item:flow.Item}},{id:2,enabled:true,mechanic:{type:'orbital-drop',location:planet,item:flow.Item}}]};
 const entries=locatedEntries(universe);
 assert.equal(entries[0].transport,'rocket-export');assert.equal(entries[1].transport,'orbital-drop');
 const model=buildNetwork({entries,index:new Map([[1,[{location:planet,flow,amount:-1},{location:space,flow,amount:1}]],[2,[{location:space,flow,amount:-1},{location:planet,flow,amount:1}]]]),visibleIds:null,solutions:new Map(),solved:false,targets:[],locatedTargets:[],externalInputs:[],implicitInputs:[],localize:(_k,n)=>n,timeScale:'seconds',mergeRecipes:true,collapse:false,showUtilities:true,onlyEnabled:false});
 assert.equal(model.edges.length,4);assert.ok(model.edges.every(e=>e.kind!=='compatibility'));
 assert.ok(model.nodes.some(n=>n.label.includes('轨道投送')));assert.ok(model.nodes.some(n=>n.label.includes('火箭发运')));
});

test('real ELK clusters locations, retains cross edges and never mutates model',async()=>{
 const elk=new ELK();
 const node=(id,location)=>({id,location,kind:'mechanic',label:id,entries:[],search:id,utility:false});
 const a={kind:'planet',name:'a'},b={kind:'planet',name:'b'},space={kind:'space'};
 const model={nodes:[node('a1',a),node('a2',a),node('s',space),node('b1',b)],edges:[{id:'1',source:'a1',target:'a2'},{id:'2',source:'a2',target:'s'},{id:'3',source:'s',target:'b1'}],solved:false};
 const original=JSON.stringify(model);
 const result=await layoutLocations(model,graph=>elk.layout(graph));
 assert.equal(result.groups.length,3);assert.equal(result.positions.size,4);
 assert.equal(result.owner.get('a1'),result.owner.get('a2'));
 assert.notEqual(result.owner.get('a1'),result.owner.get('s'));
 assert.ok([...result.positions.values()].every(p=>Number.isFinite(p.x)&&Number.isFinite(p.y)));
 const points=[...result.positions.values()];
 for(let i=0;i<points.length;i++)for(let j=i+1;j<points.length;j++)assert.ok(Math.abs(points[i].x-points[j].x)>=145||Math.abs(points[i].y-points[j].y)>=100);
 assert.equal(JSON.stringify(model),original);
 const empty=await layoutLocations({nodes:[],edges:[],solved:false},graph=>elk.layout(graph));
 assert.equal(empty.groups.length,0);
});
