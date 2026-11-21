<script lang="ts">
  import { diagnosticFrom } from "$lib/diagnostic";
  import { isEnergyFlow } from "$lib/dual-var";
  import { locationKey } from "$lib/mechanics/located";
  import { networkFlowKey, networkRate } from "$lib/mechanics/network";
  import { runtime } from "$lib/runtime/store.svelte";
  import type {
    DualVar,
    Location,
    RuntimeDiagnostic,
    SolveResult,
    UniverseSolveResult,
  } from "$lib/runtime/types";
  import BuildingSummary, {
    aggregateBomBucket,
    unwrapBomEntries,
    type BomRawEntry,
  } from "./BuildingSummary.svelte";
  import DiagnosticView from "./DiagnosticView.svelte";
  import FlowRef from "./FlowRef.svelte";
  import LocationIcon from "./LocationIcon.svelte";
  import OptimizationSummary from "./OptimizationSummary.svelte";

  type TraceMode = "producers" | "consumers" | "network" | "suggest";

  interface SolveFlowRow {
    key: string;
    flow: DualVar;
    location?: Location;
    amount: number;
  }

  let {
    solve,
    universeSolve = null,
    error = null,
    diagnostic = null,
    busy = false,
    onSuggest,
    mechanics,
    solutions,
    onTraceFlow,
    onAddExternalInput,
    onAddTarget,
    onDisableStrictSink,
  }: {
    solve: SolveResult | null;
    universeSolve?: UniverseSolveResult | null;
    error?: string | null;
    diagnostic?: RuntimeDiagnostic | null;
    busy?: boolean;
    onSuggest?: (flow: DualVar, location?: Location) => void;
    mechanics?: readonly BomRawEntry[];
    solutions?: ReadonlyMap<number, { amount: number; cost: number }>;
    onTraceFlow?: (flow: DualVar, mode: TraceMode, location?: Location) => void;
    onAddExternalInput?: (flow: DualVar, location?: Location) => void;
    onAddTarget?: (flow: DualVar, location?: Location) => void;
    onDisableStrictSink?: () => void;
  } = $props();

  let activeTab = $state<"flows" | "bom">("flows");
  let showResiduals = $state(false);
  let groupOpen = $state({
    inputs: true,
    outputs: true,
    utilities: false,
  });

  const status = $derived(solve && "solved" in solve.status ? solve.status.solved : null);
  const located = $derived(
    universeSolve?.status.status === "solved" ? universeSolve.status : null,
  );
  const failed = $derived(
    universeSolve?.status.status === "not-solved" ? universeSolve.status : null,
  );
  const ordinaryFailure = $derived(
    solve && "not-solved" in solve.status ? solve.status["not-solved"] : null,
  );
  const solved = $derived(!!status || !!located);
  const cost = $derived(located?.cost ?? status?.cost ?? 0);
  const timeScale = $derived(runtime.selectedProject?.settings.time_scale ?? "seconds");

  const effectiveSolutions = $derived.by(() => {
    if (solutions) return solutions;
    const list = located?.mechanics ?? status?.mechanics ?? [];
    return new Map(
      list.map((m) => [m.mechanic, { amount: m.amount ?? NaN, cost: m.cost ?? NaN }] as const),
    );
  });

  const canShowBom = $derived(!!(mechanics && mechanics.length > 0));
  const currentTab = $derived(activeTab === "bom" && canShowBom ? "bom" : "flows");

  const bomCount = $derived.by(() => {
    if (!canShowBom || !mechanics) return 0;
    const unwrapped = unwrapBomEntries(mechanics, effectiveSolutions);
    const bucket = aggregateBomBucket(unwrapped);
    return bucket.buildings.length || unwrapped.length;
  });

  function isResidualFlow(flow: DualVar, amount: number): boolean {
    return isEnergyFlow(flow) ? Math.abs(amount) < 1e-3 : Math.abs(amount) < 1e-6;
  }

  function isUtilityFlow(flow: DualVar): boolean {
    return isEnergyFlow(flow) || (typeof flow !== "string" && "Pollution" in flow);
  }

  const rawRows = $derived.by((): SolveFlowRow[] => {
    const source = located
      ? located.flows.map((f) => ({
          flow: f.flow.flow,
          location: f.flow.location as Location | undefined,
          amount: f.amount ?? NaN,
        }))
      : (status?.flows ?? []).map((f) => ({
          flow: f.flow,
          location: undefined as Location | undefined,
          amount: f.amount ?? NaN,
        }));

    const seen = new Map<string, number>();
    const out: SolveFlowRow[] = [];
    for (const item of source) {
      if (!Number.isFinite(item.amount) || Math.abs(item.amount) <= 1e-12) continue;
      const base = `${locationKey(item.location)}|${networkFlowKey(item.flow)}`;
      const dup = seen.get(base) ?? 0;
      seen.set(base, dup + 1);
      out.push({
        key: dup === 0 ? base : `${base}#${dup}`,
        flow: item.flow,
        location: item.location,
        amount: item.amount,
      });
    }
    return out;
  });

  const partitionedRows = $derived.by(() => {
    const inputs: SolveFlowRow[] = [];
    const outputs: SolveFlowRow[] = [];
    const utilities: SolveFlowRow[] = [];
    const residuals: SolveFlowRow[] = [];

    for (const row of rawRows) {
      if (isResidualFlow(row.flow, row.amount)) {
        residuals.push(row);
      } else if (isUtilityFlow(row.flow)) {
        utilities.push(row);
      } else if (row.amount < 0) {
        inputs.push(row);
      } else {
        outputs.push(row);
      }
    }

    const byAbsDesc = (a: SolveFlowRow, b: SolveFlowRow) =>
      Math.abs(b.amount) - Math.abs(a.amount);
    inputs.sort(byAbsDesc);
    outputs.sort(byAbsDesc);
    utilities.sort(byAbsDesc);
    residuals.sort(byAbsDesc);

    return {
      inputs,
      outputs,
      utilities,
      residuals,
      effectiveCount: inputs.length + outputs.length + utilities.length,
    };
  });

  const utilitySummary = $derived.by(() => {
    const rows = partitionedRows.utilities;
    if (rows.length === 0) return "";
    let netElectricity = 0;
    let hasElectricity = false;
    let netHeat = 0;
    let hasHeat = false;
    let pollutionCount = 0;

    for (const row of rows) {
      if (row.flow === "Electricity") {
        netElectricity += row.amount;
        hasElectricity = true;
      } else if (row.flow === "Heat") {
        netHeat += row.amount;
        hasHeat = true;
      } else if (typeof row.flow !== "string" && "Pollution" in row.flow) {
        pollutionCount += 1;
      }
    }

    const parts: string[] = [];
    if (hasElectricity && Math.abs(netElectricity) >= 1e-3) {
      parts.push(
        `电 ${netElectricity > 0 ? "+" : "-"}${networkRate("Electricity", netElectricity, timeScale)}`,
      );
    } else if (hasHeat && Math.abs(netHeat) >= 1e-3) {
      parts.push(`热 ${netHeat > 0 ? "+" : "-"}${networkRate("Heat", netHeat, timeScale)}`);
    }
    if (pollutionCount > 0) parts.push(`污染 ${pollutionCount}`);
    return parts.join(" · ");
  });

  const flowGroups = $derived(
    [
      {
        id: "inputs" as const,
        label: "净外部输入（消耗/缺口）",
        summary: "",
        rows: partitionedRows.inputs,
      },
      {
        id: "outputs" as const,
        label: "净产出与盈余",
        summary: "",
        rows: partitionedRows.outputs,
      },
      {
        id: "utilities" as const,
        label: "能源与污染",
        summary: utilitySummary,
        rows: partitionedRows.utilities,
      },
    ].filter((g) => g.rows.length > 0),
  );

  const diagnosticFlowClick = $derived(
    onSuggest ??
      (onTraceFlow
        ? (flow: DualVar, location?: Location) => onTraceFlow(flow, "suggest", location)
        : undefined),
  );

  function triggerPrimaryFlow(flow: DualVar, location?: Location) {
    if (onSuggest) onSuggest(flow, location);
    else onTraceFlow?.(flow, "suggest", location);
  }
</script>

{#snippet flowRow(row: SolveFlowRow, residual = false)}
  <div class="row-item one-line result-row" class:residual-row={residual}>
    {#if row.location}<LocationIcon location={row.location} size={20} />{/if}
    <FlowRef
      flow={row.flow}
      grow
      showQualifiers={false}
      size={20}
      flowRate={row.amount}
      rateBasis="solution"
      hint={onSuggest || onTraceFlow ? "左键/右键查看供给与消耗建议" : undefined}
      onClick={onSuggest || onTraceFlow ? () => triggerPrimaryFlow(row.flow, row.location) : undefined}
      onContextMenu={onSuggest || onTraceFlow ? () => triggerPrimaryFlow(row.flow, row.location) : undefined}
    />
    <strong
      class="mono amount"
      class:amount-pos={!residual && row.amount > 0}
      class:amount-neg={!residual && row.amount < 0}
    >
      {row.amount < 0 ? "-" : row.amount > 0 ? "+" : ""}{networkRate(row.flow, row.amount, timeScale)}
    </strong>
    {#if onTraceFlow}
      <span class="trace-actions">
        <button
          type="button"
          class="micro-btn"
          title="筛选产出此流的机制"
          aria-label="筛选产出机制"
          onclick={() => onTraceFlow(row.flow, "producers", row.location)}
        >产</button>
        <button
          type="button"
          class="micro-btn"
          title="筛选消耗此流的机制"
          aria-label="筛选消耗机制"
          onclick={() => onTraceFlow(row.flow, "consumers", row.location)}
        >耗</button>
        <button
          type="button"
          class="micro-btn"
          title="在生产网络中定位"
          aria-label="在网络中定位"
          onclick={() => onTraceFlow(row.flow, "network", row.location)}
        >图</button>
        <button
          type="button"
          class="micro-btn"
          title="查看供给与消耗建议"
          aria-label="查看建议"
          onclick={() => onTraceFlow(row.flow, "suggest", row.location)}
        >建议</button>
      </span>
    {:else if onSuggest}
      <span class="trace-actions">
        <button
          type="button"
          class="micro-btn"
          title="查看供给与消耗建议"
          aria-label="查看建议"
          onclick={() => onSuggest(row.flow, row.location)}
        >建议</button>
      </span>
    {/if}
  </div>
{/snippet}

<section class="panel solve-panel">
  <div class="solve-head">
    <div class="title">求解结果</div>
    <span class="chip" class:ok={solved} class:warn={!!error || !!failed || !!ordinaryFailure || busy}>
      {busy ? "求解中" : solved ? "已求解" : failed || ordinaryFailure ? "未求解" : "待求解"}
    </span>
  </div>

  {#if error}
    <div class="err-box">
      <strong>求解失败</strong>
      <DiagnosticView
        diagnostic={diagnostic ??
          (runtime.solveDiagnostic?.message === error
            ? runtime.solveDiagnostic
            : diagnosticFrom(error))}
        onFlowClick={diagnosticFlowClick}
        {onAddExternalInput}
        {onAddTarget}
        {onDisableStrictSink}
      />
    </div>
  {/if}

  {#if solved}
    <div class="cost-block">
      <span class="eyebrow">总成本</span>
      <strong class="mono cost-value">{cost.toFixed(3)}</strong>
      <span class="cost-note">机制与外部输入成本之和 · 结果随编辑自动刷新</span>
    </div>

    {#if (universeSolve ?? solve)?.report}
      <OptimizationSummary report={(universeSolve ?? solve)!.report} />
    {/if}

    {#if canShowBom}
      <div class="solve-tabs" role="tablist" aria-label="求解结果视图">
        <button
          type="button"
          class="solve-tab"
          class:active={currentTab === "flows"}
          role="tab"
          aria-selected={currentTab === "flows"}
          onclick={() => (activeTab = "flows")}
        >
          <span>流平衡</span>
          <span class="tab-count mono">{partitionedRows.effectiveCount}</span>
        </button>
        <button
          type="button"
          class="solve-tab"
          class:active={currentTab === "bom"}
          role="tab"
          aria-selected={currentTab === "bom"}
          onclick={() => (activeTab = "bom")}
        >
          <span>建筑清单</span>
          <span class="tab-count mono">{bomCount}</span>
        </button>
      </div>
    {:else}
      <div class="section-head">
        <span>总流平衡</span>
        <span class="count">{partitionedRows.effectiveCount} 条</span>
      </div>
    {/if}

    {#if currentTab === "bom" && canShowBom && mechanics}
      <div class="tab-body">
        <BuildingSummary
          {mechanics}
          solutions={effectiveSolutions}
          flows={rawRows}
          isUniverse={!!located}
          {timeScale}
        />
      </div>
    {:else}
      <div class="tab-body flow-groups">
        {#each flowGroups as group (group.id)}
          {@const open = groupOpen[group.id]}
          <div class="flow-group">
            <button
              type="button"
              class="group-toggle"
              aria-expanded={open}
              onclick={() => (groupOpen[group.id] = !groupOpen[group.id])}
            >
              <span class="chev">{open ? "▾" : "▸"}</span>
              <span class="group-title">{group.label}</span>
              {#if group.summary}
                <span class="group-summary mono">{group.summary}</span>
              {/if}
              <span class="count">{group.rows.length} 条</span>
            </button>
            {#if open}
              <div class="rows compact">
                {#each group.rows as row (row.key)}
                  {@render flowRow(row)}
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <div class="empty-hint">
            {partitionedRows.residuals.length > 0
              ? `无显著净流（已折叠 ${partitionedRows.residuals.length} 条浮点极微残差）`
              : "所有流已完全配平（无净流入或盈余）。"}
          </div>
        {/each}

        {#if partitionedRows.residuals.length > 0}
          <div class="residual-wrap">
            <button
              type="button"
              class="residual-toggle"
              aria-expanded={showResiduals}
              onclick={() => (showResiduals = !showResiduals)}
            >
              <span>{showResiduals ? "隐藏极微残差" : "显示极微残差"} ({partitionedRows.residuals.length})</span>
              <span class="residual-note">能量 &lt;1mW · 物料 &lt;1μ/s</span>
            </button>
            {#if showResiduals}
              <div class="rows compact residual-list">
                {#each partitionedRows.residuals as row (row.key)}
                  {@render flowRow(row, true)}
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  {:else if failed}
    <div class="err-box">
      <strong>未求解</strong>
      <DiagnosticView
        diagnostic={{
          code: "not-solved",
          message: failed.description,
          no_provider: [],
          no_consumer: [],
          located_no_provider: failed.no_provider,
          located_no_consumer: failed.no_consumer,
          flows: [],
        }}
        onFlowClick={diagnosticFlowClick}
        {onAddExternalInput}
        {onAddTarget}
        {onDisableStrictSink}
      />
    </div>
  {:else if ordinaryFailure}
    <div class="err-box">
      <strong>未求解</strong>
      <DiagnosticView
        diagnostic={{
          code: "not-solved",
          message: ordinaryFailure.description,
          no_provider: ordinaryFailure.no_provider,
          no_consumer: ordinaryFailure.no_consumer,
          flows: [],
        }}
        onFlowClick={diagnosticFlowClick}
        {onAddExternalInput}
        {onAddTarget}
        {onDisableStrictSink}
      />
    </div>
  {:else}
    <div class="empty-hint">改动目标或机制后会自动重新求解。</div>
  {/if}
</section>

<style>
  .panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }
  .solve-head, .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .solve-head {
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line);
  }
  .solve-head .title {
    margin-bottom: 0;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .section-head {
    margin-top: 4px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.04em;
  }
  .cost-block {
    display: grid;
    gap: 4px;
    margin: 14px 0 14px;
    padding: 12px 14px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius);
    box-shadow: var(--shadow-sm, var(--shadow));
  }
  .cost-value {
    font-size: 28px;
    font-weight: 750;
    line-height: 1.15;
    letter-spacing: -0.02em;
  }
  .cost-note, .empty-hint {
    font-size: 10.5px;
    color: var(--faint);
    line-height: 1.4;
  }
  .solve-tabs {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px;
    padding: 3px;
    margin-bottom: 10px;
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }
  .solve-tab {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 26px;
    padding: 3px 8px;
    color: var(--muted);
    background: transparent;
    border: 0;
    border-radius: 3px;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .solve-tab:hover {
    color: var(--text);
  }
  .solve-tab.active {
    color: var(--text);
    background: var(--card);
    box-shadow: var(--shadow-sm, none);
  }
  .tab-count {
    padding: 0 5px;
    border-radius: 999px;
    background: var(--panel);
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 9.5px;
    line-height: 1.4;
  }
  .solve-tab.active .tab-count {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .tab-body {
    min-width: 0;
  }
  .flow-groups {
    display: grid;
    gap: 10px;
  }
  .flow-group {
    display: grid;
    gap: 4px;
  }
  .group-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 2px 5px;
    border: 0;
    border-bottom: 1px solid var(--line);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .group-toggle:hover .chev {
    color: var(--text);
  }
  .chev {
    width: 9px;
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
  }
  .group-title {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    font-weight: 650;
  }
  .group-summary {
    flex: 0 0 auto;
    color: var(--muted);
    font-size: 10px;
    white-space: nowrap;
  }
  .count {
    flex: 0 0 auto;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 10px;
    color: var(--faint);
    white-space: nowrap;
  }
  .mono {
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
  }
  .rows {
    display: grid;
    gap: 2px;
    margin-top: 2px;
  }
  .row-item {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    padding: 3px 0;
  }
  .result-row {
    padding: 3px 4px;
    border-radius: var(--radius-sm);
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .result-row:hover {
    background: var(--card-hover);
  }
  .residual-row {
    opacity: 0.72;
  }
  .amount {
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
    flex: 0 0 auto;
  }
  .amount-pos { color: var(--ok); }
  .amount-neg { color: var(--text); }
  .trace-actions {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex: 0 0 auto;
  }
  .micro-btn {
    min-height: 19px;
    padding: 0 4px;
    color: var(--muted);
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 3px;
    font-size: 9.5px;
    line-height: 1.2;
    white-space: nowrap;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .micro-btn:hover {
    color: var(--text);
    background: var(--panel-elev);
    border-color: var(--line-strong);
  }
  .residual-wrap {
    margin-top: 2px;
    padding-top: 6px;
    border-top: 1px dashed var(--line);
  }
  .residual-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    padding: 4px 6px;
    color: var(--muted);
    background: var(--panel-elev);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10px;
    cursor: pointer;
    transition: color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }
  .residual-toggle:hover {
    color: var(--text);
    border-color: var(--line-strong);
  }
  .residual-note {
    color: var(--faint);
    font-size: 9.5px;
    font-family: var(--mono);
  }
  .residual-list {
    margin-top: 6px;
  }
  .err-box {
    margin-top: 12px;
    padding: 10px 12px;
    color: var(--danger);
    background: var(--danger-dim);
    border: 1px solid var(--danger-line);
    border-left: 3px solid var(--danger);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  .err-box > strong {
    display: block;
    margin-bottom: 4px;
    font-size: 12px;
    font-weight: 700;
  }
  .empty-hint {
    padding: 16px 0;
    text-align: center;
  }
</style>
