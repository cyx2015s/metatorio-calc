<script lang="ts">
  import type { DualVar, RuntimeDiagnostic } from "$lib/runtime/types";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { presentFlow } from "$lib/dual-var";
  import { normalizeSearch } from "$lib/rich-text";
  import FlowRef from "./FlowRef.svelte";
  let { diagnostic, onFlowClick }: { diagnostic: RuntimeDiagnostic; onFlowClick?: (flow: DualVar) => void } = $props();
  let query = $state("");
  let sections = $derived([
    {label: "无供给", flows: diagnostic.no_provider},
    {label: "无消耗", flows: diagnostic.no_consumer},
    {label: "关联对象", flows: diagnostic.flows},
  ]);
  let total = $derived(sections.reduce((sum, section) => sum + section.flows.length, 0));
  let needle = $derived(normalizeSearch(query));
  function matches(flow: DualVar) {
    return !needle || normalizeSearch(`${presentFlow(flow, (kind, name) => runtime.localizedName(kind, name)).description} ${JSON.stringify(flow)}`).includes(needle);
  }
</script>

<section class="diagnostic" aria-label="诊断信息" data-diagnostic-code={diagnostic.code}>
  <div class="diagnostic-message">{diagnostic.message}</div>
  {#if total}
    <input aria-label="搜索诊断对象" placeholder={`搜索 ${total} 个关联对象的名称或 ID…`} bind:value={query} />
    {#each sections as section (section.label)}
      {#if section.flows.length}
        <div class="diagnostic-section">
          <strong>{section.label} <small>{section.flows.filter(matches).length}/{section.flows.length}</small></strong>
          <div class="diagnostic-flows">
            {#each section.flows.filter(matches) as flow, i (i)}
              <FlowRef {flow} onClick={onFlowClick ? () => onFlowClick?.(flow) : undefined}
                title={onFlowClick ? "查看供给与消耗建议" : undefined} />
            {:else}<span class="muted">没有匹配的对象</span>{/each}
          </div>
        </div>
      {/if}
    {/each}
  {/if}
</section>

<style>
  .diagnostic { min-width: 0; }
  .diagnostic-message { white-space: pre-wrap; overflow-wrap: anywhere; }
  input { width: 100%; min-width: 0; margin-top: 7px; }
  .diagnostic-section { margin-top: 8px; }
  .diagnostic-section > strong { display: block; font-size: 11px; margin-bottom: 4px; }
  small { color: var(--muted); font-weight: normal; }
  .diagnostic-flows { display: flex; flex-direction: column; gap: 5px; max-height: 280px; overflow: auto; }
</style>
