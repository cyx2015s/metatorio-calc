<script lang="ts">
  import { iconTypeForKind } from "./icon-types";
  // 固定高度的机制行：左侧异构配置，右侧通用输入 / 输出。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { solarBalance } from "$lib/runtime/client";
  import FlowChip from "./FlowChip.svelte";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import SecondaryMenu from "./SecondaryMenu.svelte";
  import { groupModules } from "./mechanic-layout";
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
    onPick,
    onToggleEnabled,
    onRemove,
    onAddBeacon,
    onPickFuel,
    onClone,
    onInteractionChange,
  }: {
    entry: MechanicEntry;
    solution?: { amount: number; cost: number } | null;
    /** 当前项目/工厂 id（加载机制展开流用）。 */
    project: number;
    factory: number;
    onPick: (kind: CatalogKind | "beacon-module", a?: number, b?: number) => void;
    onToggleEnabled: () => void;
    onRemove: () => void;
    onAddBeacon: () => void;
    onPickFuel: () => void;
    onClone: () => void;
    onInteractionChange?: (active: boolean) => void;
  } = $props();

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

  let menus = $state({ modules: false, settings: false, flows: false });
  $effect(() => { onInteractionChange?.(Object.values(menus).some(Boolean)); });
  let moduleGroups = $derived(groupModules(entry.mechanic.module_config?.modules ?? []));
  let beacons = $derived(entry.mechanic.module_config?.beacons ?? []);
  let beaconCount = $derived(beacons.reduce((sum, beacon) => sum + beacon.count, 0));
  let inputs = $derived(mechanicFlows.filter((item) => item.amount < 0));
  let outputs = $derived(mechanicFlows.filter((item) => item.amount >= 0));
  let scale = $derived(solution ? Math.max(0, solution.amount) : 1);
  let hasMachine = $derived(kind === "recipe" || kind === "mining" || kind === "tile-extract");
  let hasModules = $derived(kind === "recipe" || kind === "mining");
  let hasFuel = $derived(((kind === "recipe" || kind === "mining") && machineEnergySource === "burner") || kind === "boiler" || kind === "reactor");
  let hasSettings = $derived(hasFuel || ["generator", "boiler", "reactor", "solar", "fluid-heat", "item-launch"].includes(kind));
  let amountUnit = $derived(hasMachine || ["generator", "boiler", "reactor", "solar"].includes(kind) ? "台" : "系数");
</script>

<article data-mechanic-id={entry.id} class="mech-card" class:off={!entry.enabled} aria-label={`${kindLabel[kind]}：${primaryLabel}`}>
  <div class="identity">
    <div class="configuration">
      <button class="icon-btn primary" title={`更换${kindLabel[kind]}：${primaryLabel || "未设置"}`} aria-label={`更换${kindLabel[kind]}：${primaryLabel || "未设置"}`} onclick={() => onPick(primaryKind())}>
        <HoverIcon type={primaryIcon} name={primaryName || "+"} size={28} detailKind={primaryName ? primaryKind() : undefined} quality={primaryQuality} />
      </button>
      {#if hasMachine}
        <button class="icon-btn machine-button" title={`更换机器：${machineLabel || "未选择"}`} aria-label={`更换机器：${machineLabel || "未选择"}`}
          onclick={() => onPick(kind === "tile-extract" ? "offshore-pump" : machineKind())}>
          <HoverIcon type="entity" name={machineName || "+"} size={28} detailKind={machineName ? machineKind() : undefined} quality={machineQuality} />
        </button>
      {:else if kind === "tile-dispose"}
        <button class="icon-btn" title="选择可销毁物品的地格" aria-label="选择可销毁物品的地格" onclick={() => onPick("tile")}>
          <HoverIcon type="tile" name={entry.mechanic.tile || "+"} size={26} detailKind={entry.mechanic.tile ? "tile" : undefined} />
        </button>
      {:else if kind === "generator" || kind === "boiler"}
        <button class="icon-btn" title={`流体：${fluidLabel || "未选择"}`} aria-label={`更换流体：${fluidLabel || "未选择"}`} onclick={() => onPick("fluid")}>
          <HoverIcon type="fluid" name={fluidName || "+"} size={26} detailKind={fluidName ? "fluid" : undefined} />
        </button>
      {:else if kind === "solar"}
        <button class="icon-btn" title="更换蓄电器" aria-label="更换蓄电器" onclick={() => onPick("accumulator")}>
          <HoverIcon type="entity" name={entry.mechanic.accumulator?.id || "+"} size={26} quality={entry.mechanic.accumulator?.quality} detailKind={entry.mechanic.accumulator ? "accumulator" : undefined} />
        </button>
      {/if}
      {#if hasModules}
        <SecondaryMenu title="插件与插件塔" onOpenChange={(open) => menus.modules = open}>
          {#snippet trigger(toggle, open)}
            <button class="config-preview" onclick={toggle} aria-expanded={open} aria-haspopup="dialog"
              title={`编辑插件与插件塔：${entry.mechanic.module_config?.modules.length ?? 0} 个机器插件，${beaconCount} 座塔（${beacons.length} 组配置）`}>
              {#each moduleGroups.slice(0, 2) as group}
                <span class="preview-item"><HoverIcon type="item" name={group.item.id} detailKind="module" quality={group.item.quality} size={20} /><small>×{group.count}</small></span>
              {/each}
              {#if moduleGroups.length > 2}<small>+{moduleGroups.length - 2}种</small>{/if}
              {#each beacons.slice(0, 1) as beacon}
                <span class="preview-item"><HoverIcon type="entity" name={beacon.beacon.id} detailKind="beacon" quality={beacon.beacon.quality} size={20} /><small>×{beacon.count}</small></span>
                {#each beacon.modules.slice(0, 1) as [module, count]}
                  <span class="preview-item tower-module" title="塔内插件总数"><HoverIcon type="item" name={module.id} detailKind="module" quality={module.quality} size={20} /><small>×{count}</small></span>
                {/each}
                {#if beacon.modules.length > 1}<small>+{beacon.modules.length - 1}种</small>{/if}
              {/each}
              {#if beacons.length > 1}<small>+{beacons.length - 1}组</small>{/if}
              {#if !moduleGroups.length && !beacons.length}<span>＋ 插件 / 信标</span>{/if}
              <span class="disclosure">▾</span>
            </button>
          {/snippet}
          <ModuleEditor {entry} onPickModule={(slot) => onPick("module", slot)} onPickBeacon={(beacon) => onPick("beacon", beacon)}
            onPickBeaconModule={(beacon, module) => onPick("beacon-module", beacon, module)} {onAddBeacon} />
        </SecondaryMenu>
      {/if}
      {#if hasSettings}
        <SecondaryMenu title={`${kindLabel[kind]}设置`} onOpenChange={(open) => menus.settings = open}>
          {#snippet trigger(toggle, open)}
            <button class="settings-button" onclick={toggle} aria-expanded={open} aria-haspopup="dialog" title="编辑机制参数">
              {#if entry.mechanic.fuel}<Icon type={entry.mechanic.fuel.kind === "fluid" ? "fluid" : "item"}
                name={entry.mechanic.fuel.kind === "fluid" ? entry.mechanic.fuel.fluid : entry.mechanic.fuel.item.id} size={20} />
              {/if}
              {kind === "reactor" ? `相邻 ${entry.mechanic.neighbours ?? 0}` : kind === "item-launch" ? (entry.mechanic.weight_mode ? "按重量" : "按槽位") : "参数"} ▾
            </button>
          {/snippet}

          {#if hasFuel}
            <div class="setting-line"><span>燃料</span>
              <button class="btn" onclick={onPickFuel}>{fuelLabel(entry.mechanic.fuel)}</button>
              {#if entry.mechanic.fuel}<button class="btn ghost" onclick={() => runtime.setFuel(entry.id, null).catch(() => {})}>恢复自动</button>{/if}
            </div>
          {/if}
            {#if ["generator", "boiler", "tile-extract", "reactor", "solar", "fluid-heat"].includes(kind)}
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
        </SecondaryMenu>
      {/if}
    </div>
    <div class="metadata">
      <span class="kind-label">{kindLabel[kind]}</span>
      {#if solution}
        <span class="amount mono" title={`用量 ${solution.amount} · 单位成本 ${solution.cost} · 总成本 ${solution.amount * solution.cost}`}>
          <strong>{compactNumber(solution.amount)}</strong> {amountUnit}
          <span class="cost">总成本 {compactNumber(solution.amount * solution.cost)}</span>
        </span>
      {:else}<span class="unsolved">未求解 · 单位流量</span>{/if}
      <div class="actions">
        <button class="icon-btn enabled" class:on={entry.enabled} role="switch" aria-checked={entry.enabled}
          aria-label={entry.enabled ? "停用机制" : "启用机制"} title={entry.enabled ? "已启用，点击停用" : "已停用，点击启用"} onclick={onToggleEnabled}>{entry.enabled ? "✓" : "○"}</button>
        <button class="icon-btn" title="克隆机制" aria-label="克隆机制" onclick={onClone}>⧉</button>
        <button class="icon-btn danger" title="移除机制" aria-label="移除机制" onclick={onRemove}>×</button>
      </div>
    </div>
  </div>
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
    <SecondaryMenu title={`${primaryLabel} · 完整流量`} onOpenChange={(open) => menus.flows = open}>
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
  .off .identity { opacity: .65; }
  .identity { min-width: 0; display: grid; grid-template-rows: 30px 24px; gap: 4px; padding: 6px 10px; border-right: 1px solid var(--line); }
  .configuration, .metadata { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .primary, .machine-button { flex: 0 0 auto; width: 30px; height: 30px; }
  .kind-label { color: var(--muted); font-size: 10px; white-space: nowrap; }
  .actions { display: flex; flex: 0 0 auto; gap: 0; margin-left: auto; }
  .actions .icon-btn { width: 22px; height: 24px; font-size: 12px; }
  .enabled.on { color: var(--ok); }
  .amount { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 10px; line-height: 1.1; }
  .amount strong { font-size: 12px; }
  .cost { color: var(--muted); font-size: 10px; margin-left: 6px; }
  .unsolved { color: var(--muted); font-size: 10px; }
  .config-preview, .settings-button { display: flex; align-items: center; min-width: 0; max-width: 100%; gap: 4px; padding: 1px 3px;
    height: 26px; border: 1px solid var(--line); border-radius: var(--radius-sm); color: var(--muted); background: var(--input-bg); cursor: pointer; font-size: 10px; }
  .config-preview:hover, .settings-button:hover { border-color: var(--line-strong); color: var(--text); }
  .config-preview { overflow: hidden; }
  .preview-item { display: inline-flex; align-items: center; gap: 1px; flex: 0 0 auto; }
  .preview-item small { font-family: var(--mono); font-size: 10px; }
  .settings-button { flex: 0 0 auto; white-space: nowrap; }
  .disclosure { margin-left: auto; }
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
  .row2, .row2b, .setting-line { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin: 6px 0 10px; }
  .sub { color: var(--muted); font-size: 11px; }
  .temp { display: inline-flex; gap: 5px; align-items: center; }
  .temp input { width: 70px; min-height: 28px; color: var(--text); background: var(--input-bg); border: 1px solid var(--line-strong); border-radius: var(--radius-sm); }
  .solar-balance { line-height: 1.6; }
  .spacer { flex: 1; }
  @media (max-width: 600px) { .mech-card { grid-template-columns: minmax(220px, 46%) minmax(0, 1fr); } .identity { padding-inline: 6px; } }
</style>
