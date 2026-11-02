/** Reuse unchanged JSON branches without mutating the published snapshot.
 * `next` is a fresh, owned IPC response; only that response is reconciled.
 */
export function reconcileSnapshot<T>(previous: T | null, next: T): T {
  if (Object.is(previous, next)) return next;
  if (!previous || !next || typeof previous !== 'object' || typeof next !== 'object'
      || Array.isArray(previous) !== Array.isArray(next)) return next;
  const before = previous as Record<string, unknown>;
  const after = next as Record<string, unknown>;
  const keys = Object.keys(after);
  let equal = keys.length === Object.keys(before).length;
  for (const key of keys) {
    if (!Object.hasOwn(before, key)) { equal = false; continue; }
    const old = before[key];
    const shared = reconcileSnapshot(old, after[key]);
    if (!Object.is(old, shared)) equal = false;
    after[key] = shared;
  }
  return equal ? previous : next;
}
