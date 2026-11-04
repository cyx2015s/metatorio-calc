import test from 'node:test';
import assert from 'node:assert/strict';
import {reorderIds} from '../src/lib/reorder.ts';

function fixture(ids) {
  let items = ids.map(id => ({id}));
  const moves = [];
  return {moves, read: () => items, async move(id, position) {
    moves.push({id, position});
    const next = [...items];
    next.splice(Math.min(position, next.length - 1), 0, ...next.splice(next.findIndex(e => e.id === id), 1));
    items = next;
  }};
}
function* permutations(items) {
  if (!items.length) {yield []; return;}
  for (let i = 0; i < items.length; i++) {
    for (const tail of permutations(items.filter((_, index) => index !== i))) yield [items[i], ...tail];
  }
}

test('every permutation of six entries reaches the requested order without mutating it', async () => {
  for (const order of permutations([1, 2, 3, 4, 5, 6])) {
    const f = fixture([1, 2, 3, 4, 5, 6]);
    await reorderIds(Object.freeze(order), f.read, f.move);
    assert.deepEqual(f.read().map(e => e.id), order);
  }
});
test('single-item drag in either direction uses exactly one write, unchanged lists use none', async () => {
  for (const length of [0, 1, 2, 10, 100]) {
    const initial = Array.from({length}, (_, id) => id);
    const unchanged = fixture(initial);
    await reorderIds(initial, unchanged.read, unchanged.move);
    assert.equal(unchanged.moves.length, 0);
    for (let from = 0; from < length; from++) for (let to = 0; to < length; to++) {
      const order = [...initial];
      order.splice(to, 0, ...order.splice(from, 1));
      const f = fixture(initial);
      await reorderIds(order, f.read, f.move);
      assert.deepEqual(f.read().map(e => e.id), order);
      assert.equal(f.moves.length, from === to ? 0 : 1);
    }
  }
});
test('fallback reads membership changes and snapshots the requested order before awaiting', async () => {
  const order = [4, 3, 2, 1], moves = [];
  let entries = [1, 2, 3, 4].map(id => ({id}));
  await reorderIds(order, () => entries, async (id, position) => {
    moves.push(id);
    const next = [...entries];
    next.splice(Math.min(position, next.length - 1), 0, ...next.splice(next.findIndex(e => e.id === id), 1));
    entries = next.filter(e => e.id !== 2);
    order.splice(0, order.length, 99);
  });
  assert.deepEqual(entries.map(e => e.id), [4, 3, 1]);
  assert.deepEqual(moves, [1, 3]);
});
test('a rejected move stops arbitrary reordering and preserves the original rejection', async () => {
  const failure = new Error('rejected'); let writes = 0;
  await assert.rejects(reorderIds([3, 2, 1], () => [{id: 1}, {id: 2}, {id: 3}], async () => {
    writes++; throw failure;
  }), error => error === failure);
  assert.equal(writes, 1);
});
