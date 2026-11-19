<script lang="ts">
  import { dndzone } from "svelte-dnd-action";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import type {
    DualVar,
    ExternalInput,
    FactoryDocument,
    ProjectDocument,
  } from "$lib/runtime/types";
  import FlowRef from "$lib/ui/FlowRef.svelte";
  import {
    createExternalInputShortcuts,
    DISABLED_RESOURCE_PENALTY,
    FREE_ENERGY_FLOWS,
  } from "$lib/external-input-shortcuts";
  import { flowSelectionInitial } from "$lib/mechanics/flow-selection";
  import type { MechanicFlowSelection } from "$lib/mechanics/actions";
  import { isEnergyFlow, presentFlow } from "$lib/dual-var";

  let {
    project,
    factory,
    selectFlow,
  }: {
    project: ProjectDocument;
    factory: FactoryDocument;
    selectFlow: (request: MechanicFlowSelection & { respectAccessibility?: boolean }) => void;
  } = $props();

  let open = $state(true);
  let externalInputs = $derived(factory.external_inputs ?? []);
  let implicitInputs = $derived(runtime.implicitSourcesCache);
  const inputShortcuts = $derived(
    createExternalInputShortcuts(
      {
        send: (message) => runtime.send(message),
        getFactory: (scope) =>
          runtime.document?.projects
            .find((p) => p.id === scope.project)
            ?.factories.find((f) => f.id === scope.factory) ?? null,
      },
      { project: project.id, factory: factory.id },
    ),
  );

  let addingFreeEnergy = $state(false);
  const hasFreeEnergy = $derived(
    FREE_ENERGY_FLOWS.every((flow) =>
      externalInputs.some((input) => input.flow === flow && input.penalty === 0),
    ),
  );

  async function addFreeEnergy() {
    if (addingFreeEnergy || !inputShortcuts) return;
    addingFreeEnergy = true;
    try {
      await inputShortcuts.addFreeEnergy();
    } finally {
      addingFreeEnergy = false;
    }
  }

  let implicitPromotions = $state<Record<string, boolean>>({});
  const implicitPromotionKey = (flow: DualVar) =>
    `${project.id}:${factory.id}:${JSON.stringify(flow)}`;

  async function promoteImplicitInput(flow: DualVar, penalty: number) {
    const key = implicitPromotionKey(flow),
      shortcuts = inputShortcuts;
    if (
      !shortcuts ||
      implicitPromotions[key] ||
      factory.external_inputs.some((input) => JSON.stringify(input.flow) === JSON.stringify(flow))
    )
      return;
    implicitPromotions[key] = true;
    try {
      await shortcuts.setCost(flow, penalty);
    } finally {
      delete implicitPromotions[key];
    }
  }

  let dragInputs = $state<ExternalInput[]>([]);
  $effect(() => {
    dragInputs = externalInputs;
  });

  function handleInputsConsider(event: CustomEvent<{ items: ExternalInput[] }>) {
    dragInputs = event.detail.items;
  }

  function handleInputsFinalize(event: CustomEvent<{ items: ExternalInput[] }>) {
    dragInputs = event.detail.items;
    runtime.reorderExternalInputs(event.detail.items.map((input) => input.id)).catch(() => {});
  }

  /** 项目时间刻度的数值因子（秒=1、分=60、时=3600）。内部一律按每秒，显示按当前刻度。 */
  function currentTimeFactor(): number {
    const s = project.settings.time_scale ?? "seconds";
    return s === "minutes" ? 60 : s === "hours" ? 3600 : 1;
  }

  /** 流的显示名：物品/流体/实体优先本地化名，否则内部 id。 */
  function flowLabel(flow: DualVar): string {
    return presentFlow(flow, (kind, name) => runtime.localizedName(kind, name)).description;
  }

  /** 流体流的期望温度区间；非流体返回 null。 */
  function fluidRangeOf(flow: DualVar): [number, number] | null {
    if (flow !== null && typeof flow === "object" && "Fluid" in flow) {
      return (flow as { Fluid: { temperature: [number, number] } }).Fluid.temperature;
    }
    return null;
  }

  /** 解析温度输入；非法返回 null（不把 NaN 写进文档）。 */
  function parseTemperature(raw: string): number | null {
    const value = Number(raw.trim());
    return Number.isFinite(value) ? Math.round(value) : null;
  }

  /**
   * 只改流体流的温度区间，流体名不动。`apply` 由调用方给出（目标 / 外部输入各走
   * 自己的消息），两处行为因此不会漂移。
   *
   * 区间是**期望**而不是既成事实：下游只要产出落在 [lo, hi] 内就能满足它。默认区间
   * 是流体的 [default_temperature, max_temperature]，所以新建流不会钉死在某个点上。
   */
  function setFluidRange(
    flow: DualVar,
    low: number,
    high: number,
    apply: (flow: DualVar) => void,
  ) {
    if (fluidRangeOf(flow) === null) return;
    const name = (flow as { Fluid: { name: string } }).Fluid.name;
    apply({ Fluid: { name, temperature: [Math.min(low, high), Math.max(low, high)] } });
  }

  function setExternalInputFluidRange(input: ExternalInput, low: number, high: number) {
    setFluidRange(input.flow, low, high, (flow) =>
      runtime.setExternalInputFlow(input.id, flow).catch(() => {}),
    );
  }

  function editExternalInput(input?: ExternalInput) {
    const destination = { project: project.id, factory: factory.id };
    selectFlow({
      title: input ? "更改外部输入流" : "添加外部输入流",
      respectAccessibility: false,
      ...flowSelectionInitial(input?.flow),
      onSelectFlow: (flow) =>
        runtime
          .send({
            scope: "factory",
            action: {
              ...destination,
              action: {
                "external-input": input
                  ? { "set-flow": { input: input.id, flow } }
                  : { add: { input: { id: 0, flow, penalty: 1 } } },
              },
            },
          })
          .catch(() => {}),
    });
  }
</script>

<section class="panel" aria-label="外部输入">
  <button
    class="title panel-toggle"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <span class="toggle-label">
      <span class="chev">{open ? "▾" : "▸"}</span>
      外部输入 <span class="count">{externalInputs.length}</span>
    </span>
  </button>
  {#if open}
    <div class="external-shortcuts">
      <button
        class="btn"
        onclick={() => addFreeEnergy().catch(() => {})}
        disabled={addingFreeEnergy || hasFreeEnergy || !runtime.activeContext}
        title={hasFreeEnergy
          ? "电力和热量已配置为零成本外部输入"
          : "将电力和热量设为零成本外部输入，复用已有条目"}
      >
        + 免费电力与热量
      </button>
    </div>
    <div
      class="rows"
      use:dndzone={{ items: dragInputs, flipDurationMs: 120 }}
      onconsider={handleInputsConsider}
      onfinalize={handleInputsFinalize}
    >
      {#each dragInputs as input (input.id)}
        {@const fluidRange = fluidRangeOf(input.flow)}
        <div class="row-item">
          <FlowRef
            flow={input.flow}
            grow
            showQualifiers={false}
            size={26}
            showBadge={!fluidRange}
            title="外部输入 · 点击更改"
            onClick={() => editExternalInput(input)}
          />
          {#if input.penalty === DISABLED_RESOURCE_PENALTY}<span class="chip" title="使用极高成本抑制该来源">已禁用</span>{/if}
          {#if fluidRange}
            <!-- 与目标行同一套语义：区间内任何温度的外部输入都算满足。
                 外部输入本来就是"从外面买"，指定可接受温度区间比钉死一个温度更准。 -->
            <span class="temp-range">
              <input
                class="num temp"
                type="text"
                inputmode="numeric"
                value={String(fluidRange[0])}
                title="期望温度下限（℃）"
                onchange={(event) => {
                  const next = parseTemperature((event.currentTarget as HTMLInputElement).value);
                  if (next !== null) setExternalInputFluidRange(input, next, fluidRange[1]);
                }}
              />
              <span class="temp-sep">~</span>
              <input
                class="num temp"
                type="text"
                inputmode="numeric"
                value={String(fluidRange[1])}
                title="期望温度上限（℃）"
                onchange={(event) => {
                  const next = parseTemperature((event.currentTarget as HTMLInputElement).value);
                  if (next !== null) setExternalInputFluidRange(input, fluidRange[0], next);
                }}
              />
            </span>
          {/if}
          <input
            class="num"
            type="number"
            step="0.1"
            min="0"
            title={`外部输入成本：获得 1 个「${flowLabel(input.flow)}」的代价（按项目时间刻度）`}
            aria-label={`外部输入成本：${flowLabel(input.flow)}`}
            value={isEnergyFlow(input.flow)
              ? String(Number(((input.penalty ?? NaN) * 1e6).toFixed(6)))
              : String(Number(((input.penalty ?? NaN) / currentTimeFactor()).toFixed(6)))}
            onchange={(event) => {
              const raw = (event.currentTarget as HTMLInputElement).value;
              const value = Number(raw);
              if (!Number.isFinite(value)) return;
              // 能量流：输入为"每 MW 成本"，内部存"每 W 成本"（除以 1e6）。
              // 非能量：输入为"当前单位下产出 1 个的代价"，内部存"每秒产出 1 个的代价"。
              const internal = isEnergyFlow(input.flow)
                ? value / 1e6
                : value * currentTimeFactor();
              runtime.setExternalInputPenalty(input.id, internal).catch(() => {});
            }}
          />
          <span class="row-actions">
            {#if input.penalty === DISABLED_RESOURCE_PENALTY}
              <button
                class="icon-btn"
                aria-label={`撤销禁用：${flowLabel(input.flow)}`}
                title="移除极高成本覆盖，当前环境若提供该资源则恢复免费输入"
                onclick={() => runtime.removeExternalInput(input.id).catch(() => {})}
              >↶</button>
            {/if}
            <button
              class="icon-btn danger"
              title={`移除外部输入「${flowLabel(input.flow)}」`}
              aria-label="移除外部输入"
              onclick={() => runtime.removeExternalInput(input.id).catch(() => {})}
            >×</button>
          </span>
        </div>
      {:else}
        <div class="empty-hint">还没有外部输入</div>
      {/each}
    </div>
    {#if implicitInputs.length > 0}
      <div class="implicit-hint">环境自带（隐式免费，编辑成本可转为手动输入）</div>
      <div class="rows">
        {#each implicitInputs as flow (JSON.stringify(flow))}
          <div class="row-item one-line implicit" title="当前地表或轨道的可用资源，严格供给下也免费">
            <FlowRef
              {flow}
              grow
              showQualifiers={false}
              size={24}
              title="环境自带 · 严格供给下也免费"
            />
            <span class="chip">隐式</span>
            <input
              class="num"
              type="number"
              step="0.1"
              min="0"
              value="0"
              aria-label={`外部成本：${flowLabel(flow)}`}
              title="编辑成本后保存为手动外部输入"
              disabled={implicitPromotions[implicitPromotionKey(flow)]}
              onchange={(event) => {
                const raw = event.currentTarget.value.trim();
                const value = Number(raw);
                if (raw === "" || !Number.isFinite(value) || value < 0) return;
                const internal = isEnergyFlow(flow) ? value / 1e6 : value * currentTimeFactor();
                promoteImplicitInput(flow, internal).catch(() => {});
              }}
            />
            <span class="row-actions">
              <button
                class="icon-btn"
                title={`以零成本把「${flowLabel(flow)}」保存为手动外部输入`}
                aria-label="设为手动外部输入"
                disabled={implicitPromotions[implicitPromotionKey(flow)]}
                onclick={() => promoteImplicitInput(flow, 0).catch(() => {})}
              >+</button>
              <button
                class="icon-btn danger"
                aria-label={`禁用环境资源：${flowLabel(flow)}`}
                title="将外部输入成本设为 10¹²，优先使用其他来源"
                disabled={implicitPromotions[implicitPromotionKey(flow)]}
                onclick={() => promoteImplicitInput(flow, DISABLED_RESOURCE_PENALTY).catch(() => {})}
              >⊘</button>
            </span>
          </div>
        {/each}
      </div>
    {/if}
    <button
      class="btn block"
      onclick={() => editExternalInput()}
      disabled={!runtime.activeContext}
    >+ 添加外部输入</button>
  {/if}
</section>

<style>
  .panel {
    padding: 14px 16px;
    background: transparent;
    border: 0;
    border-bottom: 0;
    border-radius: 0;
    box-shadow: none;
  }

  .external-shortcuts {
    display: flex;
    gap: 6px;
    margin: 0 0 8px;
  }

  .external-shortcuts .btn {
    flex: 1;
    font-size: 11px;
  }

  .panel-toggle {
    width: 100%;
    display: block;
    margin: 0 0 10px;
    padding: 0;
    color: var(--text);
    background: none;
    border: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .panel-toggle:hover {
    color: var(--text);
  }

  .panel-toggle:hover .chev {
    color: var(--text);
  }

  .toggle-label {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
  }

  .chev {
    width: 9px;
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 9px;
    transition: color var(--transition-fast);
  }

  .panel-toggle .count {
    margin-left: auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
    font-weight: 500;
    letter-spacing: 0;
  }

  .rows {
    display: grid;
    gap: 1px;
    margin-bottom: 10px;
  }

  .row-item {
    min-width: 0;
    min-height: 32px;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
    padding: 3px 6px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .row-item:hover,
  .row-item:focus-within {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .row-item.one-line {
    flex-wrap: nowrap;
  }

  .row-actions {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 1px;
    opacity: 0;
    transform: translateX(2px);
    transition: opacity var(--transition-fast), transform var(--transition-fast);
  }

  .row-item:hover .row-actions,
  .row-item:focus-within .row-actions {
    opacity: 1;
    transform: none;
  }

  .row-actions .icon-btn {
    width: 20px;
    height: 22px;
    font-size: 11px;
  }

  .row-item.implicit {
    border-style: dashed;
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--panel) 92%, var(--accent) 4%);
  }

  .implicit-hint {
    margin: 10px 0 4px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .num {
    width: 68px;
    min-height: 26px;
    padding: 0 6px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10.5px;
    text-align: right;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .num:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .temp-range {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    flex: 0 0 auto;
  }

  .num.temp {
    width: 38px;
    text-align: center;
  }

  .temp-sep {
    color: var(--faint);
    font-size: 9px;
  }

  .empty-hint {
    padding: 14px 4px;
    color: var(--faint);
    font-size: 11px;
    line-height: 1.55;
  }

  @media (max-width: 1360px) {
    .row-item {
      gap: 5px;
    }
    .num {
      width: 60px;
    }
    .num.temp {
      width: 32px;
    }
    .row-actions .icon-btn {
      width: 18px;
    }
    .temp-range {
      gap: 1px;
    }
  }

  @media (max-width: 1120px) {
    .row-item {
      gap: 5px;
    }
    .num {
      width: 60px;
    }
    .num.temp {
      width: 32px;
    }
    .temp-range {
      gap: 1px;
    }
  }
</style>
