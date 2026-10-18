import type { IdWithQuality } from "$lib/runtime/types";

/** Summaries count physical modules, preserving the quality of each variant. */
export function groupModules(modules: IdWithQuality[]) {
  const groups = new Map<string, { item: IdWithQuality; count: number }>();
  for (const item of modules) {
    const key = JSON.stringify([item.id, item.quality ?? "normal"]);
    const group = groups.get(key);
    if (group) group.count++;
    else groups.set(key, { item, count: 1 });
  }
  return [...groups.values()];
}
