import test from 'node:test';
import assert from 'node:assert/strict';
import {universeSolveSignatures} from '../src/lib/mechanics/universe-signature.ts';

const fixture=()=>({projects:[{id:1,settings:{solver:'microlp',time_scale:'seconds'},universes:[{id:3,name:'A',strict_source:true,strict_sink:false,mechanics:[{id:4,enabled:true,mechanic:{type:'local',location:{kind:'space'},mechanic:{type:'item-fuel',item:{id:'coal',quality:'normal'}}}}],targets:[],target_expressions:[],external_inputs:[{id:5,flow:{location:{kind:'space'},flow:'Electricity'},penalty:0},{id:6,flow:{location:{kind:'space'},flow:'Heat'},penalty:0}]},{id:7,name:'B',strict_source:true,strict_sink:false,mechanics:[],targets:[],target_expressions:[],external_inputs:[]}]}]});
test('names, input ordering and display units preserve universe solve identity',()=>{
 const document=fixture(),before=universeSolveSignatures(document);
 document.projects[0].settings.time_scale='minutes';
 document.projects[0].universes[0].name='renamed';
 document.projects[0].universes[0].external_inputs.reverse();
 assert.deepEqual(universeSolveSignatures(document),before);
});
test('numerical changes invalidate only the affected universe, while project inputs affect both',()=>{
 const document=fixture(),before=universeSolveSignatures(document);
 document.projects[0].universes[0].external_inputs[0].penalty=8;
 const after=universeSolveSignatures(document);
 assert.notEqual(after.get('1:3'),before.get('1:3'));assert.equal(after.get('1:7'),before.get('1:7'));
 document.projects[0].settings.solver='rust-certified';
 assert.notEqual(universeSolveSignatures(document).get('1:7'),after.get('1:7'));
});
