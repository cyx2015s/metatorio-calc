import test from 'node:test';
import assert from 'node:assert/strict';
import { factoryImplicitSignature, factorySolveSignatures } from '../src/lib/mechanics/factory-signature.ts';

const fixture = () => ({projects:[{id:1,context_id:'test',settings:{time_scale:'seconds',reactor_neighbours:{}},factories:[{
  id:2,name:'factory',settings:{planet:'nauvis'},strict_source:true,strict_sink:false,
  mechanics:[{id:3,enabled:true,mechanic:{type:'recipe',recipe:{id:'iron',quality:'normal'}}},{id:4,enabled:true,mechanic:{type:'solar'}}],
  targets:[{id:5,flow:'Electricity',amount:1}],external_inputs:[],target_expressions:[],
}]}]});
const key = doc => factorySolveSignatures(doc).get('1:2');

test('factory solve identity ignores names, row ordering and display time units', () => {
  const doc=fixture(), before=key(doc);
  doc.projects[0].factories[0].name='renamed';
  doc.projects[0].settings.time_scale='minutes';
  doc.projects[0].factories[0].mechanics.reverse();
  assert.equal(key(doc),before);
});
test('factory solve identity includes targets, environment, mechanisms and project coefficients', () => {
  for (const edit of [
    doc=>doc.projects[0].factories[0].targets[0].amount++,
    doc=>doc.projects[0].factories[0].settings.planet='fulgora',
    doc=>doc.projects[0].factories[0].mechanics[0].enabled=false,
    doc=>doc.projects[0].settings.reactor_neighbours.r=5,
    doc=>doc.projects[0].context_id='other',
  ]) {
    const doc=fixture(), before=key(doc);edit(doc);assert.notEqual(key(doc),before);
  }
});

test('implicit sources do not reload for target, mechanic or penalty-only edits', () => {
  const doc=fixture(), before=factoryImplicitSignature(doc,1,2),factory=doc.projects[0].factories[0];
  factory.targets[0].amount++;
  factory.mechanics[0].enabled=false;
  assert.equal(factoryImplicitSignature(doc,1,2),before);
  factory.external_inputs.push({id:6,flow:'Electricity',penalty:0});
  const withInput=factoryImplicitSignature(doc,1,2);
  assert.notEqual(withInput,before);
  factory.external_inputs[0].penalty=100;
  assert.equal(factoryImplicitSignature(doc,1,2),withInput);
});
