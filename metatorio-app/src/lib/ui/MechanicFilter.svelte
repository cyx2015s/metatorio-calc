<script lang="ts">
  import { untrack } from "svelte";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { describeFlow } from "$lib/display-labels";
  import { normalizeSearch } from "$lib/rich-text";
  import { flowObjectKey, matchesMechanic, type ExpandedFlow, type FlowFilter } from "$lib/mechanic-filter";
  import type { DualVar, MechanicEntry } from "$lib/runtime/types";

  let { project, factory, entries, expanded, onChange }: {
    project: number; factory: number; entries: MechanicEntry[]; expanded: boolean;
    onChange: (ids: number[] | null) => void;
  } = $props();
  type Side = "input" | "output";
  let input = $state<FlowFilter | null>(null);
  let output = $state<FlowFilter | null>(null);
  let picker = $state<Side | null>(null);
  let query = $state("");
  let index = $state<Map<number, ExpandedFlow[]>>(new Map());
  let failures = $state<number[]>([]);
  let completed = $state(0);
  let retry = $state(0);
  let active = $derived(!!input || !!output);
  let pending = $derived(completed < entries.length);
  const label = (flow: DualVar) => describeFlow(flow, (kind, name) => runtime.localizedName(kind, name));

  // Expand only when this feature is used, in bounded batches. Cards use the
  // same cache, including in-flight requests. Never classify a failed read as empty.
  $effect(() => {
    if ((!expanded && !active) || !project || !factory) return;
    const snapshot = entries.map(entry => ({id: entry.id, mechanic: JSON.parse(JSON.stringify(entry.mechanic))}));
    const document = runtime.document?.projects.find(entry => entry.id === project);
    JSON.stringify([document?.settings, runtime.effectiveContextId, document?.context_id, document?.factories.find(entry => entry.id === factory)?.settings]);
    void retry;
    untrack(() => { index = new Map(); failures = []; completed = 0; });
    let alive = true;
    (async () => {
      for (let offset = 0; offset < snapshot.length && alive; offset += 6) {
        const batch = await Promise.allSettled(snapshot.slice(offset, offset + 6).map(async entry => ({
          id: entry.id, flows: await runtime.loadMechanicFlow(project, factory, entry.id, entry.mechanic),
        })));
        if (!alive) return;
        const next = new Map(untrack(() => index));
        const errors = [...untrack(() => failures)];
        batch.forEach((result, i) => {
          if (result.status === "fulfilled") next.set(result.value.id, result.value.flows);
          else errors.push(snapshot[offset + i].id);
        });
        index = next;
        failures = errors;
        completed = Math.min(offset + batch.length, snapshot.length);
      }
    })();
    return () => { alive = false; };
  });

  let matching = $derived(entries.filter(entry => {
    const flows = index.get(entry.id);
    return flows !== undefined && matchesMechanic(flows, input, output);
  }).map(entry => entry.id));
  $effect(() => { onChange(active ? matching : null); });

  let candidates = $derived.by(() => {
    const objects = new Map<string, DualVar>();
    for (const flows of index.values()) for (const entry of flows) {
      if (picker === "input" ? entry.amount >= 0 : entry.amount <= 0) continue;
      const key = flowObjectKey(entry.flow);
      if (!objects.has(key)) objects.set(key, entry.flow);
    }
    const needle = normalizeSearch(query);
    return [...objects.values()].filter(flow => !needle || normalizeSearch(`${filterLabel({flow})} ${flowObjectKey(flow)}`).includes(needle));
  });

  function setFilter(side: Side, filter: FlowFilter | null) {
    if (side === "input") input = filter; else output = filter;
  }
  function valueOf(flow: DualVar): {quality?: string; temperature?: [number, number]} {
    return typeof flow === "string" ? {} : Object.values(flow)[0] as {quality?: string; temperature?: [number, number]};
  }
  function temperatures(filter: FlowFilter): [number, number][] {
    const ranges = new Map<string, [number, number]>();
    for (const flows of index.values()) for (const entry of flows) {
      const range = valueOf(entry.flow).temperature;
      if (range && flowObjectKey(entry.flow) === flowObjectKey(filter.flow)) ranges.set(JSON.stringify(range), range);
    }
    if (filter.temperature) ranges.set(JSON.stringify(filter.temperature), filter.temperature);
    return [...ranges.values()];
  }
  function filterLabel(filter: FlowFilter) {
    const flow = filter.flow;
    if (typeof flow !== "string") {
      if ("Item" in flow || "Entity" in flow) {
        const kind = "Item" in flow ? "item" : "entity";
        const value = Object.values(flow)[0] as {id: string};
        return runtime.localizedName(kind, value.id);
      }
      if ("Fluid" in flow) return runtime.localizedName("fluid", (flow.Fluid as {name: string}).name);
    }
    return label(flow);
  }
</script>

{#if expanded || active}
  <section class="mechanic-filter" aria-label="机制流筛选">
    {#if expanded}
      <div class="filter-fields">
        {#each ["input", "output"] as side}
          {@const direction = side as Side}
          {@const filter = direction === "input" ? input : output}
          <div class="filter-field">
            <span>{direction === "input" ? "消耗" : "产出"}</span>
            <button class="btn choose-flow" aria-label={`选择${direction === "input" ? "消耗" : "产出"}对象`}
              aria-expanded={picker === direction} onclick={() => { picker = picker === direction ? null : direction; query = ""; }}>
              {filter ? filterLabel(filter) : "不限 · 选择对象"}
            </button>
            {#if filter}
              {#if valueOf(filter.flow).quality !== undefined}
                <select aria-label={`${direction === "input" ? "消耗" : "产出"}品质`} value={filter.quality ?? "*"}
                  onchange={event => setFilter(direction, {...filter, quality: event.currentTarget.value === "*" ? undefined : event.currentTarget.value})}>
                  <option value="*">任意品质</option>
                  {#each runtime.catalogIndex?.qualities ?? ["normal"] as quality}<option value={quality}>{runtime.localizedName("quality", quality)}</option>{/each}
                </select>
              {:else if valueOf(filter.flow).temperature !== undefined}
                <select aria-label={`${direction === "input" ? "消耗" : "产出"}温度`} value={filter.temperature ? JSON.stringify(filter.temperature) : "*"}
                  onchange={event => setFilter(direction, {...filter, temperature: event.currentTarget.value === "*" ? undefined : JSON.parse(event.currentTarget.value)})}>
                  <option value="*">任意温度</option>
                  {#each temperatures(filter) as range}<option value={JSON.stringify(range)}>{range[0] === range[1] ? range[0] : range.join("～")}℃</option>{/each}
                </select>
              {/if}
              <button class="icon-btn" aria-label={`清除${direction === "input" ? "消耗" : "产出"}筛选`} onclick={() => setFilter(direction, null)}>×</button>
            {/if}
          </div>
        {/each}
      </div>
      {#if picker}
        <div class="flow-picker" aria-label="筛选对象候选">
          <input aria-label="搜索筛选对象" placeholder="搜索名称或内部 ID…" bind:value={query} />
          <button class="btn" onclick={() => picker = null}>关闭候选</button>
          <div class="flow-options">
            {#each candidates as flow (flowObjectKey(flow))}
              <button class="flow-option" onclick={() => { setFilter(picker!, {flow}); picker = null; }}>{filterLabel({flow})}</button>
            {:else}<span class="muted">{pending ? "正在读取机制流…" : "没有符合条件的对象"}</span>{/each}
          </div>
        </div>
      {/if}
    {/if}
    <div class="filter-status" role="status">
      {#if active}<strong>显示 {matching.length} / {entries.length} 个机制</strong>{/if}
      {#if pending}<span>正在读取 {completed} / {entries.length} · 结果逐步补齐</span>{/if}
      {#if failures.length}<span class="warn">{failures.length} 个机制读取失败，尚未判定</span><button class="btn" onclick={() => retry += 1}>重试</button>{/if}
      {#if expanded && !pending && !failures.length}<span class="muted">消耗与产出取交集 · 先筛选变体，再合并配方</span>{/if}
      {#if active}<button class="btn" onclick={() => { input = null; output = null; picker = null; }}>清除筛选</button>{/if}
    </div>
  </section>
{/if}

<style>
  .mechanic-filter { flex: 0 0 auto; padding: 8px; margin-top: 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel); }
  .filter-fields, .filter-field, .filter-status { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; }
  .filter-fields { gap: 8px 16px; }
  .filter-field > span { font-size: 12px; color: var(--muted); }
  .choose-flow { max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  select { max-width: 150px; }
  .filter-status { font-size: 12px; color: var(--muted); margin-top: 4px; }
  .filter-status strong { color: var(--text); }
  .flow-picker { margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--line); }
  .flow-picker input { width: min(260px, 70%); margin-right: 6px; }
  .flow-options { display: flex; flex-wrap: wrap; align-content: flex-start; gap: 4px; max-height: 180px; overflow: auto; margin-top: 6px; }
  .flow-option { padding: 5px 8px; border: 1px solid var(--line); border-radius: 4px; color: var(--text); background: var(--bg); cursor: pointer; }
  .flow-option:hover { border-color: var(--accent); }
</style>
