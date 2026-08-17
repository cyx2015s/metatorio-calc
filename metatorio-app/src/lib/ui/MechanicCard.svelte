<script lang="ts">
  // 单个机制卡片：主选择器（配方/资源/物品/设备）为图标按钮，
  // 机器为图标按钮，插件配置在展开区（ModuleEditor）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { mechanicFlow, solarBalance } from "$lib/runtime/client";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import ModuleEditor from "./ModuleEditor.svelte";
  import { dualVarLabel } from "$lib/runtime/types";
  import { compactNumber } from "$lib/format";
  import type { CatalogKind, MechanicEntry } from "$lib/runtime/types";

  const kindLabel: Record<string, string> = {
    recipe: "配方",
    mining: "采矿",
    spoil: "腐坏",
    plant: "种植",
    "item-fuel": "物品燃料",
    "item-launch": "发射",
    generator: "发电机",
    boiler: "锅炉",
    reactor: "反应堆",
    solar: "太阳能",
    "fluid-fuel": "流体燃料",
    "fluid-heat": "流体热",
  };

  let {
    entry,
    solution = null,
    project,
    factory,
    compact = false,
    onPick,
    onToggleEnabled,
    onRemove,
    onModuleSlot,
    onAddBeacon,
    onPickFuel,
    onClone,
  }: {
    entry: MechanicEntry;
    solution?: { amount: number; cost: number } | null;
    /** 当前项目/工厂 id（加载机制展开流用）。 */
    project: number;
    factory: number;
    /** 聚合视图下非首行的品质变体：紧凑显示（隐藏机器/细节行）。 */
    compact?: boolean;
    onPick: (kind: CatalogKind | "beacon-module", a?: number, b?: number) => void;
    onToggleEnabled: () => void;
    onRemove: () => void;
    onModuleSlot: (slot: number, module: string | null) => void;
    onAddBeacon: () => void;
    onPickFuel: () => void;
    onClone: () => void;
  } = $props();

  // 机制展开流（系数 1 时每秒产/耗；正值产出、负值消耗）。
  // 有求解结果时按实际用量缩放显示；无求解结果时显示系数 1 的原始流。
  let mechanicFlows = $state<{ flow: import("$lib/runtime/types").DualVar; amount: number }[]>([]);
  $effect(() => {
    let alive = true;
    mechanicFlow(project, factory, entry.id)
      .then((flows) => {
        if (!alive) return;
        mechanicFlows = flows.map(([flow, amount]) => ({ flow, amount }));
      })
      .catch(() => {
        if (alive) mechanicFlows = [];
      });
    return () => {
      alive = false;
    };
  });

  // 太阳能配平信息（平均出力 / 周期溢出总电量 / 蓄电器配比）
  let solarBalanceInfo = $state<import("$lib/runtime/types").SolarBalance | null>(null);
  $effect(() => {
    if (entry.mechanic.type !== "solar") {
      solarBalanceInfo = null;
      return;
    }
    let alive = true;
    solarBalance(project, factory, entry.id)
      .then((info) => {
        if (alive) solarBalanceInfo = info;
      })
      .catch(() => {
        if (alive) solarBalanceInfo = null;
      });
    return () => {
      alive = false;
    };
  });

  let kind = $derived(entry.mechanic.type);

  /** 流 → 图标 (type, name)。 */
  function flowIconOf(flow: import("$lib/runtime/types").DualVar): { type: string; name: string } {
    if (flow !== null && typeof flow === "object") {
      if ("Item" in flow) {
        const item = (flow as { Item: { id: string } }).Item;
        return { type: "item", name: item.id };
      }
      if ("Fluid" in flow) {
        const fluid = (flow as { Fluid: { name: string } }).Fluid;
        return { type: "fluid", name: fluid.name };
      }
      if ("Entity" in flow) {
        const entity = (flow as { Entity: { id: string } }).Entity;
        return { type: "entity", name: entity.id };
      }
    }
    if (typeof flow === "string") {
      if (flow === "Electricity") return { type: "flow", name: "Electricity" };
      if (flow === "Heat") return { type: "flow", name: "Heat" };
    }
    return { type: "flow", name: dualVarLabel(flow) };
  }

  /** 流 → 渲染键。 */
  function dualVarKey(flow: import("$lib/runtime/types").DualVar): string {
    return dualVarLabel(flow);
  }

  /** 流行数量文本（系数 1 或按求解用量缩放；复刻 egui compact_number）。 */
  function formatFlowQty(amount: number, scale: number): string {
    return compactNumber(Math.abs(amount) * scale);
  }
  let primaryName = $derived(
    entry.mechanic.recipe?.id ??
      entry.mechanic.item?.id ??
      entry.mechanic.seed?.id ??
      entry.mechanic.resource ??
      entry.mechanic.generator?.id ??
      entry.mechanic.boiler?.id ??
      entry.mechanic.reactor?.id ??
      entry.mechanic.solar_panel?.id ??
      entry.mechanic.fluid ??
      "",
  );
  let primaryIcon = $derived(
    kind === "recipe"
      ? "recipe"
      : kind === "mining" || kind === "generator" || kind === "boiler" || kind === "reactor" || kind === "solar"
        ? "entity"
        : kind === "fluid-fuel" || kind === "fluid-heat"
          ? "fluid"
          : "item",
  );
  let machineName = $derived(entry.mechanic.machine?.id ?? "");
  let fluidName = $derived(entry.mechanic.fluid ?? "");
  let primaryQuality = $derived(
    entry.mechanic.recipe?.quality ??
      entry.mechanic.item?.quality ??
      entry.mechanic.seed?.quality ??
      entry.mechanic.generator?.quality ??
      entry.mechanic.boiler?.quality ??
      entry.mechanic.reactor?.quality ??
      entry.mechanic.solar_panel?.quality ??
      "normal",
  );
  let machineQuality = $derived(entry.mechanic.machine?.quality ?? "normal");
  // 本地化显示名（无翻译回退内部 id）
  let primaryLabel = $derived(primaryName ? runtime.localizedName(primaryKind(), primaryName) : "");
  let machineLabel = $derived(machineName ? runtime.localizedName(machineKind(), machineName) : "");
  let fluidLabel = $derived(fluidName ? runtime.localizedName("fluid", fluidName) : "");
  function primaryKind(): CatalogKind {
    switch (kind) {
      case "recipe":
        return "recipe";
      case "mining":
        return "resource";
      case "generator":
        return "generator";
      case "boiler":
        return "boiler";
      case "reactor":
        return "reactor";
      case "solar":
        return "solar-panel";
      case "fluid-fuel":
      case "fluid-heat":
        return "fluid";
      default:
        return "item";
    }
  }

  function machineKind(): CatalogKind {
    return kind === "mining" ? "mining-machine" : "machine";
  }

  function fuelIsFluid(name: string | null | undefined): boolean {
    if (!name) return false;
    return (runtime.catalogIndex?.entries ?? []).some(
      (entry) => entry.kind === "fluid" && entry.name === name,
    );
  }

  function fuelLabel(name: string | null | undefined): string {
    if (!name) return "燃料：自动";
    return fuelIsFluid(name)
      ? runtime.localizedName("fluid", name)
      : runtime.localizedName("item", name);
  }
</script>

<div class="mech-card card" class:off={!entry.enabled}>
  <div class="row1">
    <button
      class="icon-btn"
      class:empty={!primaryName}
      title={kindLabel[kind] ?? kind}
      onclick={() => onPick(primaryKind())}
    >
      <HoverIcon
        type={primaryName ? primaryIcon : "item"}
        name={primaryName || `+ ${kindLabel[kind] ?? kind}`}
        size={30}
        detailKind={primaryName ? primaryKind() : undefined}
        quality={primaryName ? primaryQuality : undefined}
      />
    </button>

    <div class="main">
      <div class="name">
        {primaryLabel || "未设置"}
      </div>
      <div class="meta">
        <span class="chip">{kindLabel[kind] ?? kind}</span>
        {#if solution}
          <span
            class="amount mono"
            title={`精确值：用量 ${solution.amount} 台 · 成本 ${solution.cost} · 总成本 ${solution.amount * solution.cost}`}
          >
            {compactNumber(solution.amount)} 台 · 成本 {compactNumber(solution.amount * solution.cost)}
          </span>
        {/if}
      </div>
    </div>

    <button class="btn ghost" class:on={entry.enabled} onclick={onToggleEnabled} title={entry.enabled ? "停用" : "启用"}>
      {entry.enabled ? "启用" : "停用"}
    </button>
    <button class="btn ghost" onclick={onClone} title="克隆机制">克隆</button>
    <button class="btn ghost danger" onclick={onRemove} title="移除机制">移除</button>
  </div>

  {#if mechanicFlows.length > 0}
    {@const scale = solution ? Math.max(0, solution.amount) : 1}
    <div class="flow-row">
      {#each mechanicFlows as item (dualVarKey(item.flow))}
        {@const icon = flowIconOf(item.flow)}
        <span
          class="flow-chip"
          class:out={item.amount > 0}
          title={`${dualVarLabel(item.flow)} ${item.amount > 0 ? "产出" : "消耗"} ×${(Math.abs(item.amount) * scale).toFixed(2)}/s`}
        >
          <Icon type={icon.type} name={icon.name} size={16} />
          <span class="mono">{formatFlowQty(item.amount, scale)}</span>
        </span>
      {/each}
    </div>
  {/if}

  {#if !compact}
  <div class="row2">
    {#if kind === "recipe" || kind === "mining"}
      <button class="icon-btn" class:empty={!machineName} title="机器" onclick={() => onPick(machineKind())}>
        <HoverIcon
          type="entity"
          name={machineName || "machine"}
          size={24}
          detailKind={machineName ? machineKind() : undefined}
          quality={machineName ? machineQuality : undefined}
        />
      </button>
      <span class="sub">
        {machineLabel || "选择机器"}
      </span>
    {:else if kind === "generator" || kind === "boiler"}
      <button class="icon-btn" class:empty={!fluidName} title="流体" onclick={() => onPick("fluid")}>
        <HoverIcon type="fluid" name={fluidName || "fluid"} size={24} detailKind={fluidName ? "fluid" : undefined} />
      </button>
      <span class="sub">{fluidLabel || "选择流体"}</span>
      <label class="sub temp" title="输入流体温度（留空 = 默认温度）">
        温度
        <input
          type="number"
          step="1"
          value={entry.mechanic.temperature ?? ""}
          placeholder="默认"
          onchange={(event) => {
            const raw = (event.currentTarget as HTMLInputElement).value;
            const value = raw === "" ? null : Number(raw);
            if (value === null || Number.isFinite(value)) {
              runtime.setMechanicTemperature(entry.id, value).catch(() => {});
            }
          }}
        />
      </label>
    {:else if kind === "reactor"}
      <label class="sub temp" title="相邻反应堆数量（0-8）">
        相邻
        <input
          type="number"
          min="0"
          max="8"
          step="1"
          value={entry.mechanic.neighbours ?? 0}
          onchange={(event) => {
            const value = Number((event.currentTarget as HTMLInputElement).value);
            if (Number.isFinite(value) && value >= 0 && value <= 8) {
              runtime.setNeighbours(entry.id, value).catch(() => {});
            }
          }}
        />
      </label>
    {:else if kind === "solar"}
      <button class="icon-btn" class:empty={!entry.mechanic.accumulator?.id} title="蓄电器" onclick={() => onPick("accumulator")}>
        <HoverIcon
          type="entity"
          name={entry.mechanic.accumulator?.id || "accumulator"}
          size={24}
          detailKind={entry.mechanic.accumulator?.id ? "accumulator" : undefined}
          quality={entry.mechanic.accumulator?.quality}
        />
      </button>
      <span class="sub">
        {entry.mechanic.accumulator?.id
          ? runtime.localizedName("entity", entry.mechanic.accumulator.id)
          : "选择蓄电器"}
      </span>
      {#if solarBalanceInfo}
        <span class="sub solar-balance" title={`峰值 ${compactNumber(solarBalanceInfo.peak_power)} J/s · 周期 ${solarBalanceInfo.cycle_seconds}s`}>
          平均 {compactNumber(solarBalanceInfo.average_power)} J/s
          · 溢出 {compactNumber(solarBalanceInfo.surplus_per_cycle)} J
          · 建议 {solarBalanceInfo.recommended_accumulators.toFixed(2)} 蓄电器/面板
        </span>
      {/if}
    {:else if kind === "fluid-fuel" || kind === "fluid-heat"}
      <label class="sub temp" title="流体温度（留空 = 默认温度）">
        温度
        <input
          type="number"
          step="1"
          value={entry.mechanic.temperature ?? ""}
          placeholder="默认"
          onchange={(event) => {
            const raw = (event.currentTarget as HTMLInputElement).value;
            const value = raw === "" ? null : Number(raw);
            if (value === null || Number.isFinite(value)) {
              runtime.setMechanicTemperature(entry.id, value).catch(() => {});
            }
          }}
        />
      </label>
    {/if}

    <span class="spacer"></span>
  </div>

  <!-- 机制细节：燃料 / 燃料温度 / 火箭重量模式 -->
  {#if kind === "recipe" || kind === "mining" || kind === "boiler" || kind === "reactor" || kind === "item-launch"}
    <div class="row2b">
      {#if kind === "recipe" || kind === "mining" || kind === "boiler" || kind === "reactor"}
        <button
          class="btn"
          title="指定燃料（默认 = 按燃料类别抽象选择）"
          onclick={onPickFuel}
        >{fuelLabel(entry.mechanic.fuel)}</button>
        {#if entry.mechanic.fuel}
          <button
            class="btn ghost"
            title="清除燃料（回到自动）"
            onclick={() => runtime.setFuel(entry.id, null).catch(() => {})}
          >×</button>
        {/if}
        {#if kind === "boiler"}
          <button
            class="btn"
            title="锅炉模式：箱内加热（产抽象热，原型缺省）或输出到独立管道（水→蒸汽）。点击循环切换。"
            onclick={() => {
              const cycle: Array<"heat-fluid-inside" | "output-to-separate-pipe" | null> = [
                "output-to-separate-pipe",
                "heat-fluid-inside",
                null,
              ];
              const index = cycle.indexOf(entry.mechanic.mode ?? null);
              const next = cycle[(index + 1) % cycle.length];
              runtime.setBoilerMode(entry.id, next).catch(() => {});
            }}
          >{entry.mechanic.mode === "output-to-separate-pipe"
            ? "独立管道"
            : entry.mechanic.mode === "heat-fluid-inside"
              ? "箱内加热"
              : "原型模式"}</button>
        {/if}
        {#if (kind === "recipe" || kind === "mining" || kind === "boiler") && fuelIsFluid(entry.mechanic.fuel)}
          <label class="sub temp" title="燃料流体温度（留空 = 默认温度）">
            燃料温度
            <input
              type="number"
              step="1"
              value={entry.mechanic.fuel_temperature ?? ""}
              placeholder="默认"
              onchange={(event) => {
                const raw = (event.currentTarget as HTMLInputElement).value;
                const value = raw === "" ? null : Number(raw);
                if (value === null || Number.isFinite(value)) {
                  runtime.setFuelTemperature(entry.id, value).catch(() => {});
                }
              }}
            />
          </label>
        {/if}
      {/if}
      {#if kind === "item-launch"}
        <button
          class="btn"
          class:on={entry.mechanic.weight_mode ?? false}
          title="火箭运力模式：按堆叠槽位 或 按重量"
          onclick={() =>
            runtime.setWeightMode(entry.id, !(entry.mechanic.weight_mode ?? false)).catch(() => {})}
        >{entry.mechanic.weight_mode ? "按重量" : "按堆叠槽位"}</button>
      {/if}
      <span class="spacer"></span>
    </div>
  {/if}

  {#if kind === "recipe" || kind === "mining"}
    <div class="row3">
      <ModuleEditor
        {entry}
        onPickModule={(slot) => onPick("module", slot)}
        onPickBeacon={(beacon) => onPick("beacon", beacon)}
        onPickBeaconModule={(beacon, module) => onPick("beacon-module", beacon, module)}
        {onAddBeacon}
      />
    </div>
  {/if}
  {/if}
</div>

<style>
  .mech-card {
    display: grid;
    gap: 8px;
    padding: 8px 10px;
  }

  .mech-card.off {
    opacity: 0.55;
  }

  .row1,
  .row2 {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .row2 {
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }

  .flow-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    padding-top: 2px;
  }

  .flow-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px 1px 2px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10px;
  }

  .flow-chip.out {
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--card) 88%, var(--accent) 5%);
  }

  .flow-chip .mono {
    color: var(--muted);
  }

  .row2b {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    min-height: 24px;
  }

  .row3 {
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }

  .main {
    min-width: 0;
    flex: 1;
  }

  .name {
    overflow: hidden;
    font-size: 12px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 3px;
  }

  .amount {
    color: var(--accent);
    font-size: 10px;
  }

  .sub {
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub.temp {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
  }

  .sub.temp input {
    width: 56px;
    min-height: 22px;
    padding: 0 4px;
    text-align: right;
    background: var(--card);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }

  .solar-balance {
    font-family: var(--mono);
    font-size: 10px;
  }

  .spacer {
    flex: 1;
  }
</style>
