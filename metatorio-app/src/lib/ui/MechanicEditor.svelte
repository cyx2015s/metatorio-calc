<script lang="ts">
  import {transportLabel} from "$lib/mechanics/located";
  import LocationPicker from "./LocationPicker.svelte";
  import { iconTypeForKind } from "./icon-types";
  // Shared mechanism configuration and actions; no flow expansion or flow UI.
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { solarBalance } from "$lib/runtime/client";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import SecondaryMenu from "./SecondaryMenu.svelte";
  import { groupModules } from "./mechanic-layout";
  import ModuleEditor from "./ModuleEditor.svelte";
  import { mechanicKindLabels as kindLabel } from "$lib/mechanics/presentation";
  import { compactNumber } from "$lib/format";
  import type { MechanicActions } from "$lib/mechanics/actions";
  import type { CatalogKind, MechanicEntry } from "$lib/runtime/types";



  let {
    entry,
    solution = null,
    project,
    factory,
    actions,
    onInteractionChange,
  }: {
    entry: MechanicEntry;
    solution?: { amount: number; cost: number } | null;
    /** 当前项目/工厂作用域（用于太阳能配平等环境计算）。 */
    project: number;
    factory: number;
    actions: MechanicActions;
    onInteractionChange?: (active: boolean) => void;
  } = $props();

  const onPick = (kind: CatalogKind | 'beacon-module', a?: number, b?: number) => actions.pick(entry.id, kind, a, b);
  const onToggleEnabled = () => actions.commands.setMechanicEnabled(entry.id, !entry.enabled).catch(() => {});
  const onRemove = () => actions.commands.removeMechanic(entry.id).catch(() => {});
  const onClone = () => actions.commands.cloneMechanic(entry.id).catch(() => {});
  const onAddBeacon = () => actions.addBeacon(entry.id);
  const onPickFuel = () => actions.pickFuel(entry.id);

  // 太阳能配平信息（平均出力 / 周期溢出总电量 / 蓄电器配比）
  let solarBalanceInfo = $state<import("$lib/runtime/types").SolarBalance | null>(null);
  $effect(() => {
    if (entry.mechanic.type !== "solar") {
      solarBalanceInfo = null;
      return;
    }
    // Environment changes must invalidate the displayed solar calculation too.
    const settings = runtime.document?.projects.find(p => p.id === project)?.factories.find(f => f.id === factory)?.settings;
    void settings;
    void entry.location; void entry.mechanic.solar_panel; void entry.mechanic.accumulator;
    void runtime.selectedProject?.settings;
    let alive = true;
    solarBalanceInfo = null;
    const request = actions.commands.scope.kind === "universe"
      ? runtime.loadUniversePreview(project, factory).then(preview => preview.mechanics.find(m=>m.id===entry.id)?.solar_balance ?? null)
      : solarBalance(project, factory, entry.id);
    request.then((info) => {
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
  let solarOrbit = $derived(entry.location?.kind === "space" || runtime.document?.projects.find(p => p.id === project)?.factories.find(f => f.id === factory)?.settings.surface != null);
  let solarContinuous = $derived(solarOrbit || !!solarBalanceInfo?.continuous_daylight);

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
  const neighbourEstimate = $derived(runtime.document?.projects.find(p=>p.id===project)?.settings.reactor_neighbours?.[primaryName] ?? 3);
  let machineLabel = $derived(machineName ? runtime.localizedName(machineKind(), machineName) : "");
  let fluidLabel = $derived(fluidName ? runtime.localizedName("fluid", fluidName) : "");
  function primaryKind(): CatalogKind {
    switch (kind) {
      case "recipe":
        return "recipe";
      case "mining":
        return "resource";
      case "generator":
      case "fusion-generator":
        return kind;
      case "boiler":
        return "boiler";
      case "reactor":
      case "fusion-reactor":
        return kind;
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

  let menus = $state({ modules: false, settings: false });
  $effect(() => { onInteractionChange?.(Object.values(menus).some(Boolean)); });
  let moduleGroups = $derived(groupModules(entry.mechanic.module_config?.modules ?? []));
  let beacons = $derived(entry.mechanic.module_config?.beacons ?? []);
  let beaconCount = $derived(beacons.reduce((sum, beacon) => sum + beacon.count, 0));
  let hasMachine = $derived(kind === "recipe" || kind === "mining" || kind === "tile-extract");
  let hasModules = $derived(kind === "recipe" || kind === "mining");
  let hasFuel = $derived(((kind === "recipe" || kind === "mining") && machineEnergySource === "burner") || kind === "boiler" || (kind === "reactor" || kind === "fusion-reactor"));
  let hasSettings = $derived(!entry.transport && (hasFuel || ["generator", "fusion-generator", "boiler", "reactor", "fusion-reactor", "solar", "fluid-heat", "item-launch"].includes(kind)));
  let amountUnit = $derived(entry.transport ? "件/s" : hasMachine || ["generator", "fusion-generator", "boiler", "reactor", "fusion-reactor", "solar"].includes(kind) ? "台" : "系数");
</script>

<div class="identity" class:off={!entry.enabled} data-mechanic-editor={entry.id} aria-label={`${entry.transport ? transportLabel(entry.transport) : kindLabel[kind]}：${primaryLabel}`}>
    <div class="configuration">
      {#if entry.location}<LocationPicker value={entry.location} planetsOnly={!!entry.transport} label="机制地点" onChange={location => actions.setLocation(entry.id, location).catch(() => {})} />{/if}
      <button class="icon-btn primary" title={`更换${entry.transport ? transportLabel(entry.transport) : kindLabel[kind]}：${primaryLabel || "未设置"}`} aria-label={`更换${entry.transport ? transportLabel(entry.transport) : kindLabel[kind]}：${primaryLabel || "未设置"}`} onclick={() => onPick(primaryKind())}>
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
      {:else if kind === "solar" && !solarContinuous}
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
              {#if !moduleGroups.length && !beacons.length}<span>＋ 插件 / 插件塔</span>{/if}
              <span class="disclosure">▾</span>
            </button>
          {/snippet}
          <ModuleEditor {entry} {actions} onPickModule={(slot) => onPick("module", slot)} onPickBeacon={(beacon) => onPick("beacon", beacon)}
            onPickBeaconModule={(beacon, module) => onPick("beacon-module", beacon, module)} {onAddBeacon} />
        </SecondaryMenu>
      {/if}
      {#if hasSettings}
        <SecondaryMenu title={`${entry.transport ? transportLabel(entry.transport) : kindLabel[kind]}设置`} onOpenChange={(open) => menus.settings = open}>
          {#snippet trigger(toggle, open)}
            <button class="settings-button" onclick={toggle} aria-expanded={open} aria-haspopup="dialog" title="编辑机制参数">
              {#if entry.mechanic.fuel}<Icon type={entry.mechanic.fuel.kind === "fluid" ? "fluid" : "item"}
                name={entry.mechanic.fuel.kind === "fluid" ? entry.mechanic.fuel.fluid : entry.mechanic.fuel.item.id} size={20} />
              {/if}
              {(kind === "reactor" || kind === "fusion-reactor") ? `相邻 ${entry.mechanic.neighbours ?? neighbourEstimate}` : kind === "item-launch" ? (entry.mechanic.weight_mode ? "按重量" : "按槽位") : "参数"} ▾
            </button>
          {/snippet}

          {#if hasFuel}
            <div class="setting-line"><span>燃料</span>
              <button class="btn" onclick={onPickFuel}>{fuelLabel(entry.mechanic.fuel)}</button>
              {#if entry.mechanic.fuel}<button class="btn ghost" onclick={() => actions.commands.setFuel(entry.id, null).catch(() => {})}>恢复自动</button>{/if}
            </div>
          {/if}
            {#if ["generator", "fusion-generator", "boiler", "tile-extract", "reactor", "fusion-reactor", "solar", "fluid-heat"].includes(kind)}
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
                        actions.commands.setMechanicTemperature(entry.id, value).catch(() => {});
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
                          actions.commands.setBoilerOutputTemperature(entry.id, value).catch(() => {});
                        }
                      }}
                    />
                  </label>
                {/if}
              {:else if kind === "fusion-generator"}
                <label class="sub temp" title="等离子体输入温度；留空使用原型默认温度">
                  输入温度 <input aria-label="聚变发电机输入温度" type="number" step="1" value={entry.mechanic.temperature ?? ""} placeholder="默认"
                    onchange={event=>{const raw=event.currentTarget.value;const value=raw===""?null:Number(raw);if(value===null||Number.isInteger(value))actions.commands.setMechanicTemperature(entry.id,value).catch(()=>{});}} />
                </label>
              {:else if kind === "tile-extract"}
                <button class="icon-btn" class:empty={!machineName} title={`抽水机：${machineLabel || "未选择"} · 点击更换`} onclick={() => onPick("offshore-pump")}>
                  <HoverIcon type="entity" name={machineName || "offshore-pump"} size={24} quality={machineQuality} />
                </button>
                <span class="sub">{machineLabel || "选择抽水机"}</span>
                <button class="btn ghost" title="更换抽水机" onclick={() => onPick("offshore-pump")}>更换</button>
              {:else if (kind === "reactor" || kind === "fusion-reactor")}
                <label class="sub temp" title="相邻反应堆数量；留空继承项目估计">
                  相邻
                  <input
                    type="number"
                    min="0"
                    max="255"
                    step="1"
                    aria-label="反应堆邻居数"
                    placeholder={`项目估计 ${neighbourEstimate}`}
                    value={entry.mechanic.neighbours ?? ""}
                    onchange={(event) => {
                      const raw = (event.currentTarget as HTMLInputElement).value;
                      const value = raw === "" ? null : Number(raw);
                      if (value === null || (Number.isInteger(value) && value >= 0 && value <= 255)) {
                        actions.commands.setNeighbours(entry.id, value).catch(() => {});
                      }
                    }}
                  />
                </label>
              {:else if kind === "solar"}
                {#if !solarContinuous}<button
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
                </span>{/if}
                {#if solarBalanceInfo}
                  <span class="sub solar-balance" title={`峰值 ${compactNumber(solarBalanceInfo.peak_power)} J/s · 周期 ${solarBalanceInfo.cycle_seconds}s`}>
                    {#if solarContinuous}
                      持续发电 {compactNumber(solarBalanceInfo.average_power)} W · 无需蓄电器
                    {:else}
                      平均 {compactNumber(solarBalanceInfo.average_power)} W
                      · 储能 {compactNumber(solarBalanceInfo.surplus_per_cycle)} J
                      · 建议 {solarBalanceInfo.recommended_accumulators.toFixed(2)} 蓄电器/面板
                    {/if}
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
                        actions.commands.setMechanicTemperature(entry.id, value).catch(() => {});
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
                      actions.commands.setWeightMode(entry.id, !(entry.mechanic.weight_mode ?? false)).catch(() => {})}
                  >{entry.mechanic.weight_mode ? "按重量" : "按堆叠槽位"}</button>
                {:else if kind === "boiler" || (kind === "reactor" || kind === "fusion-reactor")}
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
                            actions.commands.setFuelTemperature(entry.id, value).catch(() => {});
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
      <span class="kind-label">{entry.transport ? transportLabel(entry.transport) : kindLabel[kind]}</span>
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

<style>
  .identity { min-width: 0; display: grid; grid-template-rows: 30px 24px; gap: 4px; padding: 6px 10px; }
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
  .row2, .row2b, .setting-line { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin: 6px 0 10px; }
  .sub { color: var(--muted); font-size: 11px; }
  .temp { display: inline-flex; gap: 5px; align-items: center; }
  .temp input { width: 70px; min-height: 28px; color: var(--text); background: var(--input-bg); border: 1px solid var(--line-strong); border-radius: var(--radius-sm); }
  .solar-balance { line-height: 1.6; }
  .spacer { flex: 1; }
  .identity.off { opacity: .65; }
  @media (max-width: 600px) { .identity { padding-inline: 6px; } }
</style>
