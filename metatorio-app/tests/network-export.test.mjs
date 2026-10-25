import test from 'node:test';
import assert from 'node:assert/strict';
import {networkPngSize, NETWORK_PNG_MAX_SIDE, NETWORK_PNG_MAX_PIXELS} from '../src/lib/mechanics/network-export.ts';

test('small full-network exports retain their original resolution',()=>{
  assert.deepEqual(networkPngSize(1234,567),{width:1234,height:567,scale:1,reduced:false});
});
test('large and elongated universe exports respect both canvas side and pixel budgets',()=>{
  for(const [width,height] of [[1000000,500000],[1000000,100],[20000,20000],[1,1000000]]) {
    const size=networkPngSize(width,height);
    assert.ok(size.reduced);
    assert.ok(size.width>0&&size.height>0);
    assert.ok(size.width<=NETWORK_PNG_MAX_SIDE&&size.height<=NETWORK_PNG_MAX_SIDE);
    assert.ok(size.width*size.height<=NETWORK_PNG_MAX_PIXELS);
  }
});
test('invalid layout bounds produce a useful error before allocating a canvas',()=>{
  for(const size of [[0,1],[NaN,5],[5,Infinity],[-1,5]]) assert.throws(()=>networkPngSize(...size),/边界无效/);
});
