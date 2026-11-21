<script lang="ts">
  import type { DualVar, Location, RuntimeDiagnostic } from "$lib/runtime/types";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { presentFlow } from "$lib/dual-var";
  import { normalizeSearch } from "$lib/rich-text";
  import FlowRef from "./FlowRef.svelte";
  import LocationIcon from "./LocationIcon.svelte";

  let {
    diagnostic,
    onFlowClick,
    onAddExternalInput,
    onAddTarget,
    onDisableStrictSink,
  }: {
    diagnostic: RuntimeDiagnostic;
    onFlowClick?: (flow: DualVar, location?: Location) => void;
    onAddExternalInput?: (flow: DualVar, location?: Location) => void;
    onAddTarget?: (flow: DualVar, location?: Location) => void;
    onDisableStrictSink?: () => void;
  } = $props();

  let query = $state("");
  let sections = $derived([
    {
      kind: "no_provider" as const,
      label: "无供给",
      flows: [
        ...diagnostic.no_provider.map((flow) => ({ flow, location: undefined as Location | undefined })),
        ...(diagnostic.located_no_provider ?? []),
      ],
    },
    {
      kind: "no_consumer" as const,
      label: "无消耗",
      flows: [
        ...diagnostic.no_consumer.map((flow) => ({ flow, location: undefined as Location | undefined })),
        ...(diagnostic.located_no_consumer ?? []),
      ],
    },
    {
      kind: "flows" as const,
      label: "关联对象",
      flows: diagnostic.flows.map((flow) => ({ flow, location: undefined as Location | undefined })),
    },
  ]);
  let total = $derived(sections.reduce((sum, section) => sum + section.flows.length, 0));
  let needle = $derived(normalizeSearch(query));

  function matches(row: { flow: DualVar; location?: Location }) {
    const flow = row.flow;
    return (
      !needle ||
      normalizeSearch(
        `${presentFlow(flow, (kind, name) => runtime.localizedName(kind, name)).description} ${JSON.stringify(flow)} ${JSON.stringify(row.location)}`,
      ).includes(needle)
    );
  }
</script>

<section class="diagnostic" aria-label="诊断信息" data-diagnostic-code={diagnostic.code}>
  <div class="diagnostic-message">{diagnostic.message}</div>
  {#if total}
    <input aria-label="搜索诊断对象" placeholder={`搜索 ${total} 个关联对象的名称或 ID…`} bind:value={query} />
    {#each sections as section (section.label)}
      {#if section.flows.length}
        {@const visibleFlows = section.flows.filter(matches)}
        <div class="diagnostic-section">
          <div class="diagnostic-section-head">
            <strong>
              <span>{section.label}</span>
              <small>{visibleFlows.length}/{section.flows.length}</small>
            </strong>
            {#if section.kind === "no_consumer" && onDisableStrictSink}
              <button
                type="button"
                class="diag-btn warn"
                title="关闭严格消耗约束，允许未在目标中的产出盈余"
                onclick={() => onDisableStrictSink()}
              >关闭严格消耗</button>
            {/if}
          </div>
          <div class="diagnostic-flows">
            {#each visibleFlows as flow, i (i)}
              <div class="located-row">
                {#if flow.location}<LocationIcon location={flow.location} size={22} />{/if}
                <FlowRef
                  flow={flow.flow}
                  grow
                  onClick={onFlowClick ? () => onFlowClick?.(flow.flow, flow.location) : undefined}
                  title={onFlowClick ? "查看供给与消耗建议" : undefined}
                />
                {#if (section.kind === "no_provider" && (onAddExternalInput || onFlowClick)) || (section.kind === "no_consumer" && (onAddTarget || onFlowClick))}
                  <span class="diag-actions">
                    {#if section.kind === "no_provider" && onAddExternalInput}
                      <button
                        type="button"
                        class="diag-btn"
                        title="添加到外部输入"
                        onclick={() => onAddExternalInput(flow.flow, flow.location)}
                      >+ 输入</button>
                    {/if}
                    {#if section.kind === "no_consumer" && onAddTarget}
                      <button
                        type="button"
                        class="diag-btn"
                        title="添加到生产目标"
                        onclick={() => onAddTarget(flow.flow, flow.location)}
                      >+ 目标</button>
                    {/if}
                    {#if onFlowClick}
                      <button
                        type="button"
                        class="diag-btn"
                        title="查看供给与消耗建议"
                        onclick={() => onFlowClick(flow.flow, flow.location)}
                      >建议</button>
                    {/if}
                  </span>
                {/if}
              </div>
            {:else}<span class="muted">没有匹配的对象</span>{/each}
          </div>
        </div>
      {/if}
    {/each}
  {/if}
</section>

<style>
  .located-row {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .located-row:hover {
    background: color-mix(in srgb, var(--danger-dim) 50%, var(--card-hover));
  }
  .diagnostic { min-width: 0; }
  .diagnostic-message { color: var(--text); font-size: 11.5px; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
  input {
    width: 100%;
    min-width: 0;
    min-height: 28px;
    margin-top: 8px;
    padding: 0 8px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      box-shadow var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  input::placeholder { color: var(--faint); }
  input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }
  .diagnostic-section {
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--danger-line);
  }
  .diagnostic-section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    margin-bottom: 6px;
  }
  .diagnostic-section-head > strong {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--danger);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  small {
    color: var(--muted);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-weight: normal;
  }
  .diagnostic-flows { display: flex; flex-direction: column; gap: 3px; max-height: 280px; overflow: auto; }
  .diag-actions {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex: 0 0 auto;
    margin-left: auto;
  }
  .diag-btn {
    min-height: 20px;
    padding: 1px 6px;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 550;
    line-height: 1.2;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .diag-btn:hover {
    background: var(--card-hover);
    border-color: var(--accent-line);
  }
  .diag-btn.warn {
    color: var(--warn);
    background: var(--warn-dim);
    border-color: var(--warn-line);
  }
  .diag-btn.warn:hover {
    background: color-mix(in srgb, var(--warn) 16%, var(--warn-dim));
    border-color: var(--warn);
  }
</style>
