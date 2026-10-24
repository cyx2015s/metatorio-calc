import test from 'node:test';
import assert from 'node:assert/strict';
import { formatStat, detailStats, statLabels } from '../src/lib/ui/hover-model.ts';

test('item weights use mass units rather than power or unitless SI prefixes', () => {
  for (const [value, expected] of [[0,'0 g'],[500,'500 g'],[1500,'1.5 kg'],[1000000,'1 t']]) {
    assert.equal(formatStat({key:'item-weight',value,unit:'grams'}), expected);
  }
  assert.equal(formatStat({key:'item-weight',value:Infinity,unit:'grams'}),'—');
  assert.equal(statLabels['rocket-item-capacity'],'标准火箭载量（件）');
});

test('resolved weight facts survive quality selection without applying a quality multiplier', () => {
  const detail = {calculation:{stats:[{key:'item-weight',value:500,unit:'grams'}],qualities:[{quality:'rare',stats:[{key:'science-value',value:2,unit:'number'}]}]}};
  for (const quality of ['normal','rare','legendary']) {
    assert.equal(detailStats(detail,quality).find(stat => stat.key==='item-weight').value,500);
  }
});
