<script lang="ts">
  import { iconTypeForKind } from "./icon-types";
  // 单个机制卡片：主选择器（配方/资源/物品/设备）为图标按钮，
  // 机器为图标按钮，插件配置在展开区（ModuleEditor）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { solarBalance } from "$lib/runtime/client";
  import FlowChip from "./FlowChip.svelte";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import ModuleEditor from "./ModuleEditor.svelte";
  import { compactNumber } from "$lib/format";
  import type { CatalogKind, MechanicEntry } from "$lib/runtime/types";

  const kindLabel: Record<string, string> = {
    recipe: "配方",
    mining: "采矿",
    spoil: "变质",
    plant: "种植",
    "item-fuel": "物品燃料",
    "virtual-science": "虚拟科研",
    "item-launch": "发射",
    generator: "发电机",
    boiler: "锅炉",
    reactor: "反应堆",
    solar: "太阳能",
    "fluid-fuel": "流体燃料",
    "fluid-heat": "流体热",
    "tile-extract": "地格抽取",
    "tile-dispose": "地格销毁",
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
  // 依赖 entry.mechanic 整体：配方/机器/插件配置变化时重新加载。
  let mechanicFlows = $state<{ flow: import("$lib/runtime/types").DualVar; amount: number }[]>([]);
  $effect(() => {
    const mechanic = entry.mechanic;
    let alive = true;
    runtime
      .getMechanicFlow(project, factory, entry.id, mechanic)
      .then((flows) => {
        if (!alive) return;
        mechanicFlows = flows;
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

  /** 流 → 渲染键（含品质与完整形状：同物品不同品质、多燃料类别/多温度
   *  展开出的不同流是不同条目，避免 each key 冲突）。 */
  function dualVarKey(flow: import("$lib/runtime/types").DualVar): string {
    const item = flow !== null && typeof flow === "object" ? (flow as { Item?: { id: string; quality?: string } }).Item : undefined;
    if (item) return `item:${item.id}:${item.quality ?? "normal"}`;
    return JSON.stringify(flow);
  }
  let primaryName = $derived(
    entry.mechanic.recipe?.id ??
      entry.mechanic.item?.id ??
      entry.mechanic.tile ??
      entry.mechanic.seed?.id ??
      entry.mechanic.resource ??
      entry.mechanic.generator?.id ??
      entry.mechanic.boiler?.id ??
      entry.mechanic.reactor?.id ??
      entry.mechanic.solar_panel?.id ??
      entry.mechanic.fluid ??
      "",
  );
  let primaryIcon = $derived(iconTypeForKind(primaryKind()));
  let machineName = $derived(entry.mechanic.machine?.id ?? "");
  let fluidName = $derived(entry.mechanic.fluid ?? "");

  // 机器能量源类型（electric/burner/fluid/heat/void）：burner 机器需要物品燃料，
  // electric 等不需要——据此决定 recipe/mining 卡是否显示燃料配置。
  let machineEnergySource = $state<string | null>(null);
  $effect(() => {
    const id = machineName;
    if (!id || (kind !== "recipe" && kind !== "mining")) {
      machineEnergySource = null;
      return;
    }
    let alive = true;
    runtime.getDetail(machineKind(), id).then((detail) => {
      if (alive) machineEnergySource = detail?.machine_energy_source ?? null;
    });
    return () => {
      alive = false;
    };
  });

  // 锅炉工作模式：决定是否显示"输出温度"编辑框，也用于在卡片上标注模式。
  // heat-fluid-inside 连续加热同一流体、输出温度可任选；output-to-separate-pipe
  // 的输出温度由原型 target_temperature 固定，没有可编辑的输出温度。
  let boilerMode = $state<string | null>(null);
  // 锅炉的流体能量源是否"烧燃料"：true 时出力只取决于热值、与温度无关，
  // 因此不显示"燃料温度"输入框（那个温度只在利用流体热时才有意义）。
  let boilerBurnsFluid = $state<boolean | null>(null);
  $effect(() => {
    const id = entry.mechanic.boiler?.id;
    if (kind !== "boiler" || !id) {
      boilerMode = null;
      boilerBurnsFluid = null;
      return;
    }
    let alive = true;
    runtime.getDetail("boiler", id).then((detail) => {
      if (alive) {
        boilerMode = detail?.boiler_mode ?? null;
        boilerBurnsFluid = detail?.burns_fluid ?? null;
      }
    });
    return () => {
      alive = false;
    };
  });
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
      case "tile-extract":
        return "tile";
      case "tile-dispose":
        return "item";
      default:
        return "item";
    }
  }

  function machineKind(): CatalogKind {
    if (kind === "mining") return "mining-machine";
    // 抽水机（offshore pump）不在制造机目录里，按实体名做本地化。
    if (kind === "tile-extract") return "entity";
    return "machine";
  }

  /** 燃料显示名：Fuel::Item → 物品（带品质），Fuel::Fluid → 流体。 */
  function fuelLabel(fuel: import("$lib/runtime/types").Fuel | null | undefined): string {
    if (!fuel) return "燃料：自动";
    if (fuel.kind === "fluid") return runtime.localizedName("fluid", fuel.fluid) || fuel.fluid;
    return runtime.localizedName("item", fuel.item.id) || fuel.item.id;
  }

  /** 指定燃料是否为流体（决定温度输入是否显示）。 */
  function fuelIsFluid(fuel: import("$lib/runtime/types").Fuel | null | undefined): boolean {
    return fuel?.kind === "fluid";
  }

  /** 流体燃料温度（Fuel::Fluid.temperature）；非流体/空 → 空串。 */
  function fuelTemperature(fuel: import("$lib/runtime/types").Fuel | null | undefined): string {
    if (fuel?.kind !== "fluid") return "";
    return fuel.temperature == null ? "" : String(fuel.temperature);
  }
</script>

<div class="mech-card card" class:off={!entry.enabled}>
  <div class="row1">
    <button
      class="icon-btn"
      class:empty={!primaryName}
      title={`${kindLabel[kind] ?? kind}：${primaryLabel || "未设置"} · 点击更换`}
      aria-label={`更换${kindLabel[kind] ?? kind}`}
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

    {#if !compact && (kind === "recipe" || kind === "mining")}
      <button
        class="icon-btn"
        class:empty={!machineName}
        title={machineName ? `机器：${machineLabel} · 点击更换` : "选择机器"}
        aria-label="更换机器"
        onclick={() => onPick(machineKind())}
      >
        <HoverIcon
          type="entity"
          name={machineName || "machine"}
          size={30}
          detailKind={machineName ? machineKind() : undefined}
          quality={machineName ? machineQuality : undefined}
        />
      </button>
    {/if}

    <div class="main">
      <div class="name">
        {primaryLabel || "未设置"}
      </div>
      <div class="meta">
        <span class="chip">{kindLabel[kind] ?? kind}</span>
        {#if !compact && (kind === "recipe" || kind === "mining" || kind === "tile-extract") && machineLabel}
          <span class="chip muted">{machineLabel}</span>
        {/if}
        {#if !compact && kind === "boiler" && boilerMode}
          <span class="chip muted" title="锅炉工作模式">
            {boilerMode === "heat-fluid-inside" ? "加热流体" : "产出独立管道"}
          </span>
        {/if}
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

    {#if !compact && (((kind === "recipe" || kind === "mining") && machineEnergySource === "burner") || kind === "boiler" || kind === "reactor")}
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
    {/if}

    <button
      class="toggle-pill"
      class:on={entry.enabled}
      role="switch"
      aria-checked={entry.enabled}
      title={entry.enabled ? "已启用 · 点击停用该机制" : "已停用 · 点击启用该机制"}
      onclick={onToggleEnabled}
    >
      <span class="toggle-dot"></span>
      {entry.enabled ? "已启用" : "已停用"}
    </button>
    <span class="card-actions">
      <button class="icon-btn" title="克隆该机制（复制配方 / 机器 / 插件配置）" aria-label="克隆机制" onclick={onClone}>⧉</button>
      <button class="icon-btn danger" title="移除该机制" aria-label="移除机制" onclick={onRemove}>×</button>
    </span>
  </div>

  {#if mechanicFlows.length > 0}
    {@const scale = solution ? Math.max(0, solution.amount) : 1}
    <div class="flow-row">
      {#each mechanicFlows as item (dualVarKey(item.flow))}
        <FlowChip flow={item.flow} amount={item.amount} {scale} />
      {/each}
    </div>
  {/if}

  {#if !compact}
  {#if kind !== "recipe" && kind !== "mining"}
  <div class="row2">
    {#if kind === "generator" || kind === "boiler"}
      <button class="icon-btn" class:empty={!fluidName} title={`流体：${fluidLabel || "未选择"} · 点击更换`} onclick={() => onPick("fluid")}>
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
      {#if kind === "boiler" && boilerMode === "heat-fluid-inside"}
        <label class="sub temp" title="heat-fluid-inside 模式的输出温度（留空 = 流体最高温度）">
          输出温度
          <input
            type="number"
            step="1"
            value={entry.mechanic.output_temperature ?? ""}
            placeholder="最高"
            onchange={(event) => {
              const raw = (event.currentTarget as HTMLInputElement).value;
              const value = raw === "" ? null : Number(raw);
              if (value === null || Number.isFinite(value)) {
                runtime.setBoilerOutputTemperature(entry.id, value).catch(() => {});
              }
            }}
          />
        </label>
      {/if}
    {:else if kind === "tile-extract"}
      <button class="icon-btn" class:empty={!machineName} title={`抽水机：${machineLabel || "未选择"} · 点击更换`} onclick={() => onPick("offshore-pump")}>
        <HoverIcon type="entity" name={machineName || "offshore-pump"} size={24} quality={machineQuality} />
      </button>
      <span class="sub">{machineLabel || "选择抽水机"}</span>
      <button class="btn ghost" title="更换抽水机" onclick={() => onPick("offshore-pump")}>更换</button>
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
      <button
        class="icon-btn"
        class:empty={!entry.mechanic.accumulator?.id}
        title={entry.mechanic.accumulator?.id
          ? `蓄电器：${runtime.localizedName("entity", entry.mechanic.accumulator.id)} · 点击更换`
          : "选择蓄电器"}
        onclick={() => onPick("accumulator")}
      >
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
    {:else if kind === "fluid-heat"}
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
  {/if}

  <!-- 机制细节：燃料 / 燃料温度 / 火箭重量模式（recipe/mining 的燃料已在头部行） -->
  {#if kind === "item-launch" || (kind === "boiler" && fuelIsFluid(entry.mechanic.fuel) && boilerBurnsFluid !== true)}
    <div class="row2b">
      {#if kind === "item-launch"}
        <button
          class="btn"
          class:on={entry.mechanic.weight_mode ?? false}
          title="火箭运力模式：按堆叠槽位 或 按重量"
          onclick={() =>
            runtime.setWeightMode(entry.id, !(entry.mechanic.weight_mode ?? false)).catch(() => {})}
        >{entry.mechanic.weight_mode ? "按重量" : "按堆叠槽位"}</button>
      {:else if kind === "boiler" || kind === "reactor"}
        {#if kind === "boiler" && fuelIsFluid(entry.mechanic.fuel)}
          <label class="sub temp" title="燃料流体温度（留空 = 默认温度）">
            燃料温度
            <input
              type="number"
              step="1"
              value={fuelTemperature(entry.mechanic.fuel)}
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
      <span class="spacer"></span>
    </div>
  {/if}

  {#if kind === "recipe" || kind === "mining"}
    <details class="advanced">
      <summary title="展开/收起插件与插件塔配置">
        <span class="advanced-caret"></span>
        <span>插件与插件塔</span>
        <span class="advanced-meta">
          {#if (entry.mechanic.module_config?.modules.length ?? 0) > 0}
            {entry.mechanic.module_config?.modules.length} 插件
          {:else}
            无插件
          {/if}
          {#if (entry.mechanic.module_config?.beacons.length ?? 0) > 0}
            · {entry.mechanic.module_config?.beacons.length} 插件塔
          {/if}
        </span>
      </summary>
      <div class="row3">
        <ModuleEditor
          {entry}
          onPickModule={(slot) => onPick("module", slot)}
          onPickBeacon={(beacon) => onPick("beacon", beacon)}
          onPickBeaconModule={(beacon, module) => onPick("beacon-module", beacon, module)}
          {onAddBeacon}
        />
      </div>
    </details>
  {/if}
  {/if}
</div>

<style>
  .mech-card {
    display: grid;
    gap: 7px;
    padding: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    transition: border-color 100ms ease, background 100ms ease;
  }

  .mech-card:hover {
    border-color: var(--line-strong);
  }

  .mech-card.off {
    opacity: 0.64;
    filter: grayscale(0.35);
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
    padding-top: 1px;
  }

  .row2b {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    min-height: 22px;
  }

  .row3 {
    padding-top: 9px;
    margin-top: 1px;
    border-top: 1px solid var(--line);
  }

  .main {
    min-width: 0;
    flex: 1;
  }

  .name {
    overflow: hidden;
    font-size: 12.5px;
    font-weight: 650;
    letter-spacing: 0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 4px;
  }

  .amount {
    color: var(--ok);
    font-size: 10px;
    font-weight: 550;
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
    width: 58px;
    min-height: 24px;
    padding: 0 5px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
    text-align: right;
  }

  .sub.temp input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .solar-balance {
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9.5px;
  }

  .spacer {
    flex: 1;
  }

  /* 启用开关：状态明确，点击即切换；与「克隆/移除」图标动作分离。 */
  .toggle-pill {
    flex: 0 0 auto;
    height: 24px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    color: var(--muted);
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    transition: color 90ms ease, background 90ms ease, border-color 90ms ease;
  }

  .toggle-pill:hover {
    color: var(--text);
    background: var(--card-hover);
    border-color: var(--line-strong);
  }

  .toggle-pill .toggle-dot {
    width: 6px;
    height: 6px;
    flex: 0 0 auto;
    background: var(--faint);
    border-radius: 50%;
  }

  .toggle-pill.on {
    color: var(--ok);
    background: var(--ok-dim);
    border-color: var(--ok-line);
  }

  .toggle-pill.on .toggle-dot {
    background: var(--ok);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ok) 18%, transparent);
  }

  .card-actions {
    display: inline-flex;
    align-items: center;
    gap: 1px;
  }

  .card-actions .icon-btn {
    width: 24px;
    height: 24px;
    font-size: 12px;
  }

  /* 高级配置默认收起，避免每张卡都被插件槽撑高。 */
  .advanced {
    margin-top: 1px;
  }

  .advanced > summary {
    min-height: 26px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 2px;
    color: var(--muted);
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 600;
    list-style: none;
    cursor: pointer;
    user-select: none;
  }

  .advanced > summary::-webkit-details-marker {
    display: none;
  }

  .advanced > summary:hover {
    color: var(--text);
    background: var(--card-hover);
  }

  .advanced-caret {
    width: 0;
    height: 0;
    border-top: 4px solid transparent;
    border-bottom: 4px solid transparent;
    border-left: 5px solid currentColor;
    transition: transform 90ms ease;
  }

  .advanced[open] .advanced-caret {
    transform: rotate(90deg);
  }

  .advanced-meta {
    margin-left: auto;
    color: var(--faint);
    font-family: var(--mono);
    font-size: 9px;
    font-weight: 500;
  }

</style>
