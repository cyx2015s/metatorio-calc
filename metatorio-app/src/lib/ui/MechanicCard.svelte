<script lang="ts">
  import {transportLabel} from "$lib/mechanics/located";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { untrack } from 'svelte';
  import { mechanicPrototype } from "$lib/display-labels";
  import { mechanicKindLabels as kindLabel } from "$lib/mechanics/presentation";
  import type { MechanicEntry } from "$lib/runtime/types";
  import type { MechanicActions } from "$lib/mechanics/actions";
  import type { FlowTraceHandler } from "$lib/mechanics/workspace";
  import FlowChip from "./FlowChip.svelte";
  import LocationIcon from "./LocationIcon.svelte";
  import SecondaryMenu from "./SecondaryMenu.svelte";
  import MechanicEditor from "./MechanicEditor.svelte";

  let {entry, solution = null, project, factory, actions, onInteractionChange, onTraceFlow, onRevealInNetwork}: {
    entry: MechanicEntry; solution?: {amount: number; cost: number} | null;
    project: number; factory: number; actions: MechanicActions;
    onInteractionChange?: (active: boolean) => void;
    onTraceFlow?: FlowTraceHandler;
    onRevealInNetwork?: (mechanicId: number) => void;
  } = $props();
  let editorActive = $state(false), flowsOpen = $state(false);
  $effect(() => { onInteractionChange?.(editorActive || flowsOpen); });
  const ref = $derived(mechanicPrototype(entry.mechanic));
  const primaryLabel = $derived(runtime.localizedName(ref.kind, ref.name));
  const kind = $derived(entry.mechanic.type);
  // 机制展开流（系数 1 时每秒产/耗；正值产出、负值消耗）。
  // 有求解结果时按实际用量缩放显示；无求解结果时显示系数 1 的原始流。
  // 依赖 entry.mechanic 整体：配方/机器/插件配置变化时重新加载。
  type KeyedFlow = import("$lib/mechanic-filter").ExpandedFlow & { key: string };
  let loadingFlows = $state(true);
  let mechanicFlows = $state<KeyedFlow[]>([]);
  const environmentKey = $derived(runtime.mechanicEnvironmentKey(actions.commands.scope));
  $effect(() => {
    const mechanic = entry.mechanic;
    void entry.location;
    void environmentKey; void runtime.flowRevision;
    loadingFlows = true;
    let alive = true;
    untrack(() => actions.loadFlow(entry.id, mechanic))
      .then((flows) => {
        if (!alive) return;
        mechanicFlows = flows.map(item => ({
          ...item,
          key: JSON.stringify([item.location, dualVarKey(item.flow)]),
        }));
        loadingFlows = false;
      }).catch(() => { if (alive) { mechanicFlows = []; loadingFlows = false; } });
    return () => {
      alive = false;
    };
  });

  /** 流 → 渲染键（含品质与完整形状：同物品不同品质、多燃料类别/多温度
   *  展开出的不同流是不同条目，避免 each key 冲突）。 */
  function dualVarKey(flow: import("$lib/runtime/types").DualVar): string {
    const item = flow !== null && typeof flow === "object" ? (flow as { Item?: { id: string; quality?: string } }).Item : undefined;
    if (item) return `item:${item.id}:${item.quality ?? "normal"}`;
    return JSON.stringify(flow);
  }
  const inputs = $derived(mechanicFlows.filter(item => item.amount < 0));
  const outputs = $derived(mechanicFlows.filter(item => item.amount >= 0));
  const scale = $derived(solution ? Math.max(0, solution.amount) : 1);
</script>

<article data-mechanic-id={entry.id} class="mech-card" class:off={!entry.enabled} aria-label={`${entry.transport ? transportLabel(entry.transport) : kindLabel[kind]}：${primaryLabel}`}>
  <MechanicEditor {entry} {solution} {project} {factory} {actions} {onRevealInNetwork} onInteractionChange={active => editorActive = active} />
  <div class="flow-lanes" aria-label="机制流量">
    {#each [{ label: "输入", items: inputs }, { label: "输出", items: outputs }] as lane}
      <div class="flow-lane">
        <span class="lane-label">{lane.label}<small>{lane.items.length || ""}</small></span>
        <!-- Scroll regions must be keyboard focusable, including when they contain no buttons. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="flow-strip" tabindex="0" role="region" aria-label={`${lane.label}流量，可横向滚动`}>
          {#each lane.items as item (item.key)}
            {#if entry.transport && item.location}<LocationIcon location={item.location} size={18} />{/if}<FlowChip flow={item.flow} amount={item.amount} {scale} solved={solution !== null} location={item.location ?? entry.location} {onTraceFlow} />
          {:else}<span class="no-flow">{loadingFlows ? "加载中…" : "—"}</span>{/each}
        </div>
      </div>
    {/each}
    <SecondaryMenu title={`${primaryLabel} · 完整流量`} onOpenChange={(open) => flowsOpen = open}>
      {#snippet trigger(toggle, open)}<button class="all-flows" title="查看完整输入输出及流名称" aria-label="查看完整输入输出及流名称" onclick={toggle} aria-expanded={open} aria-haspopup="dialog">↗</button>{/snippet}
      <div class="full-flow-list">
        <p>{solution ? "当前求解用量下的流量" : "未求解：以下为单位机制流量"}</p>
        {#each [{ label: "输入", items: inputs }, { label: "输出", items: outputs }] as lane}
          <h4>{lane.label}（{lane.items.length}）</h4>
          {#each lane.items as item (item.key)}{#if entry.transport && item.location}<LocationIcon location={item.location} size={18} />{/if}<FlowChip flow={item.flow} amount={item.amount} {scale} solved={solution !== null} showName location={item.location ?? entry.location} {onTraceFlow} />{/each}
        {/each}
      </div>
    </SecondaryMenu>
  </div>
</article>

<style>
  .mech-card {
    height: var(--mechanic-row-height, 84px);
    min-height: var(--mechanic-row-height, 84px);
    overflow: hidden;
    display: grid;
    grid-template-columns: clamp(248px, 38%, 360px) minmax(0, 1fr);
    padding: 0;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .mech-card:hover {
    border-color: var(--line-strong);
    background: var(--panel-elev);
  }
  .off {
    background: var(--card);
  }
  .mech-card.off:hover {
    background: var(--card-hover);
  }
  .mech-card > :global(.identity) { border-right: 1px solid var(--line); }
  .flow-lanes { position: relative; min-width: 0; display: grid; grid-template-rows: 1fr 1fr; padding: 0 30px 0 8px; gap: 2px; }
  .flow-lane { display: flex; align-items: center; min-width: 0; gap: 6px; }
  .lane-label {
    flex: 0 0 auto;
    min-width: 36px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 2px 5px;
    color: var(--faint);
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: 3px;
    font-size: 9.5px;
    font-weight: 600;
    line-height: 1.2;
    white-space: nowrap;
  }
  .lane-label small {
    color: var(--muted);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 9.5px;
  }
  .lane-label small:empty { display: none; }
  .flow-strip { display: flex; flex: 1; align-items: center; gap: 4px; min-width: 0; height: 100%; min-height: 28px; overflow-x: auto; overflow-y: hidden; }
  .flow-strip :global(.flow-chip) { flex: 0 0 auto; }
  .no-flow { color: var(--faint); font-size: 11px; }
  .flow-lanes :global(.menu-anchor) { position: absolute; top: 50%; transform: translateY(-50%); right: 4px; }
  .all-flows {
    width: 22px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--input-bg);
    color: var(--muted);
    font-size: 12px;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .all-flows:hover {
    color: var(--text);
    background: var(--card-hover);
    border-color: var(--line-strong);
  }
  .full-flow-list { display: grid; gap: 6px; }
  .full-flow-list p { color: var(--muted); margin: 0; font-size: 11px; }
  .full-flow-list h4 {
    margin: 8px 0 0;
    padding-top: 6px;
    border-top: 1px solid var(--line);
    color: var(--faint);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  @media (max-width: 600px) { .mech-card { grid-template-columns: minmax(220px, 46%) minmax(0, 1fr); } }
</style>
