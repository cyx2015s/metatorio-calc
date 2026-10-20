<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { mechanicPrototype } from "$lib/display-labels";
  import { mechanicKindLabels as kindLabel } from "$lib/mechanics/presentation";
  import type { MechanicEntry } from "$lib/runtime/types";
  import type { MechanicActions } from "$lib/mechanics/actions";
  import FlowChip from "./FlowChip.svelte";
  import SecondaryMenu from "./SecondaryMenu.svelte";
  import MechanicEditor from "./MechanicEditor.svelte";

  let {entry, solution = null, project, factory, actions, onInteractionChange}: {
    entry: MechanicEntry; solution?: {amount: number; cost: number} | null;
    project: number; factory: number; actions: MechanicActions;
    onInteractionChange?: (active: boolean) => void;
  } = $props();
  let editorActive = $state(false), flowsOpen = $state(false);
  $effect(() => { onInteractionChange?.(editorActive || flowsOpen); });
  const ref = $derived(mechanicPrototype(entry.mechanic));
  const primaryLabel = $derived(runtime.localizedName(ref.kind, ref.name));
  const kind = $derived(entry.mechanic.type);
  // 机制展开流（系数 1 时每秒产/耗；正值产出、负值消耗）。
  // 有求解结果时按实际用量缩放显示；无求解结果时显示系数 1 的原始流。
  // 依赖 entry.mechanic 整体：配方/机器/插件配置变化时重新加载。
  let loadingFlows = $state(true);
  let mechanicFlows = $state<{ flow: import("$lib/runtime/types").DualVar; amount: number }[]>([]);
  $effect(() => {
    const mechanic = entry.mechanic;
    loadingFlows = true;
    let alive = true;
    runtime
      .getMechanicFlow(project, factory, entry.id, mechanic)
      .then((flows) => {
        if (!alive) return;
        mechanicFlows = flows;
        loadingFlows = false;
      });
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

<article data-mechanic-id={entry.id} class="mech-card" class:off={!entry.enabled} aria-label={`${kindLabel[kind]}：${primaryLabel}`}>
  <MechanicEditor {entry} {solution} {project} {factory} {actions} onInteractionChange={active => editorActive = active} />
  <div class="flow-lanes" aria-label="机制流量">
    {#each [{ label: "输入", items: inputs }, { label: "输出", items: outputs }] as lane}
      <div class="flow-lane">
        <span class="lane-label">{lane.label}<small>{lane.items.length || ""}</small></span>
        <!-- Scroll regions must be keyboard focusable, including when they contain no buttons. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="flow-strip" tabindex="0" role="region" aria-label={`${lane.label}流量，可横向滚动`}>
          {#each lane.items as item (dualVarKey(item.flow))}
            <FlowChip flow={item.flow} amount={item.amount} {scale} solved={solution !== null} />
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
          {#each lane.items as item (dualVarKey(item.flow))}<FlowChip flow={item.flow} amount={item.amount} {scale} solved={solution !== null} showName />{/each}
        {/each}
      </div>
    </SecondaryMenu>
  </div>
</article>

<style>

  .mech-card { height: var(--mechanic-row-height); min-height: var(--mechanic-row-height); display: grid;
    grid-template-columns: clamp(248px, 38%, 360px) minmax(0, 1fr); padding: 0;
    color: var(--text); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); }
  .mech-card:hover { border-color: var(--line-strong); }
  .off { background: var(--card); }
  .mech-card > :global(.identity) { border-right: 1px solid var(--line); }
  .flow-lanes { position: relative; min-width: 0; display: grid; grid-template-rows: 1fr 1fr; padding: 0 30px 0 6px; gap: 2px; }
  .flow-lane { display: flex; align-items: center; min-width: 0; gap: 6px; }
  .lane-label { flex: 0 0 28px; color: var(--muted); font-size: 10px; text-align: center; }
  .lane-label small { margin-left: 2px; font-family: var(--mono); font-size: 10px; }
  .flow-strip { display: flex; flex: 1; align-items: center; gap: 4px; min-width: 0; height: 34px; overflow-x: auto; overflow-y: hidden; }
  .flow-strip :global(.flow-chip) { flex: 0 0 auto; }
  .no-flow { color: var(--muted); font-size: 11px; }
  .flow-lanes :global(.menu-anchor) { position: absolute; top: 23px; right: 4px; }
  .all-flows { width: 22px; height: 24px; padding: 0; border: 1px solid var(--line); border-radius: var(--radius-sm); background: none; color: var(--muted); font-size: 12px; cursor: pointer; }
  .all-flows:hover { color: var(--text); border-color: var(--line-strong); }
  .full-flow-list { display: grid; gap: 6px; }
  .full-flow-list p { color: var(--muted); margin: 0; font-size: 11px; }
  .full-flow-list h4 { margin: 7px 0 0; font-size: 12px; }
  @media (max-width: 600px) { .mech-card { grid-template-columns: minmax(220px, 46%) minmax(0, 1fr); } }
</style>
