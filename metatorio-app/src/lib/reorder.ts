/** Reorder a scoped list, using one write for a normal single-item drag. */
export async function reorderIds(
  requested: readonly number[],
  read: () => readonly {id: number}[],
  move: (id: number, position: number) => Promise<unknown>,
): Promise<void> {
  const order = [...requested];
  const current = read().map(entry => entry.id);
  if (current.length === order.length) {
    const first = current.findIndex((id, index) => id !== order[index]);
    if (first < 0) return;
    let last = current.length - 1;
    while (current[last] === order[last]) last--;
    if (current[first] === order[last]
      && current.slice(first + 1, last + 1).every((id, offset) => id === order[first + offset])) {
      await move(current[first], last);
      return;
    }
    if (current[last] === order[first]
      && current.slice(first, last).every((id, offset) => id === order[first + offset + 1])) {
      await move(current[last], first);
      return;
    }
  }
  // 任意置换保留稳定的从后往前移动算法；每次写入后重新读取，因为快照或列表成员可能已变化。
  for (let position = order.length - 1; position >= 0; position--) {
    const index = read().findIndex(entry => entry.id === order[position]);
    if (index >= 0 && index !== position) await move(order[position], position);
  }
}
