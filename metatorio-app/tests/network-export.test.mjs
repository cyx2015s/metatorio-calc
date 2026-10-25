import test from 'node:test';
import assert from 'node:assert/strict';
import {networkPngSize,networkPngTiles} from '../src/lib/mechanics/network-export.ts';

test('small full-network exports retain their original resolution',()=>{
  assert.deepEqual(networkPngSize(1234,567),{width:1234,height:567});
});

test('tiles cover the entire image in native row-major order including partial edges',()=>{
  assert.deepEqual([...networkPngTiles(2053,515,2048,512)],[
    {x:0,y:0,width:2048,height:512},{x:2048,y:0,width:5,height:512},
    {x:0,y:512,width:2048,height:3},{x:2048,y:512,width:5,height:3}
  ]);
  const first=networkPngTiles(1000000,500000,2048,512).next().value;
  assert.deepEqual(first,{x:0,y:0,width:2048,height:512});
  assert.throws(()=>[...networkPngTiles(5,5,0,1)],/分区尺寸无效/);
});
test('large exports have no application-imposed side or area limits',()=>{
  for(const [width,height] of [[25288,7092],[1000000,500000],[1000000,100],[20000,20000],[1,1000000]]) {
    assert.deepEqual(networkPngSize(width,height),{width,height});
  }
});
test('invalid layout bounds produce a useful error before allocating a canvas',()=>{
  for(const size of [[0,1],[NaN,5],[5,Infinity],[-1,5]]) assert.throws(()=>networkPngSize(...size),/边界无效/);
});
