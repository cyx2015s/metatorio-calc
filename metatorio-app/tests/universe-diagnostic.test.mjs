import test from 'node:test';
import assert from 'node:assert/strict';
import {diagnosticFrom,diagnosticText} from '../src/lib/diagnostic.ts';

test('located planning diagnostics preserve location and flow through native errors and copy text',()=>{
  const flow={location:{kind:'planet',name:'nauvis'},flow:{Item:{id:'plate',quality:'rare'}}};
  const original={code:'auto-plan-not-solved',message:'backend unavailable',located_no_provider:[flow],located_no_consumer:[],no_provider:[],no_consumer:[],flows:[]};
  for(const wire of [original,JSON.stringify(original)]){
    const diagnostic=diagnosticFrom(wire);
    assert.equal(diagnostic.message,original.message);
    assert.deepEqual(diagnostic.located_no_provider,[flow]);
    const copied=diagnosticText(diagnostic,(_kind,name)=>name);
    assert.ok(copied.includes('nauvis'));assert.ok(copied.includes('plate'));assert.ok(copied.includes('rare'));
  }
});
test('invalid located diagnostic references are discarded without inferring IDs from prose',()=>{
  const diagnostic=diagnosticFrom({code:'bad',message:'planet:ghost',located_no_provider:[{location:{kind:'planet'},flow:'Electricity'},{location:{kind:'space'},flow:{Item:{id:3}}}]});
  assert.equal(diagnostic.located_no_provider,undefined);
  assert.equal(diagnostic.message,'planet:ghost');
});
