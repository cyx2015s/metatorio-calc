<script lang="ts">
  // 单个机制卡片：主选择器（配方/资源/物品/设备）为图标按钮，
  // 机器为图标按钮，插件配置在展开区（ModuleEditor）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import HoverIcon from "./HoverIcon.svelte";
  import ModuleEditor from "./ModuleEditor.svelte";
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
    "fluid-fuel": "流体燃料",
    "fluid-heat": "流体热",
  };

  let {
    entry,
    solution = null,
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
    onPick: (kind: CatalogKind | "beacon-module", a?: number, b?: number) => void;
    onToggleEnabled: () => void;
    onRemove: () => void;
    onModuleSlot: (slot: number, module: string | null) => void;
    onAddBeacon: () => void;
    onPickFuel: () => void;
    onClone: () => void;
  } = $props();

  let kind = $derived(entry.mechanic.type);
  let primaryName = $derived(
    entry.mechanic.recipe?.id ??
      entry.mechanic.item?.id ??
      entry.mechanic.seed?.id ??
      entry.mechanic.resource ??
      entry.mechanic.generator?.id ??
      entry.mechanic.boiler?.id ??
      entry.mechanic.reactor?.id ??
      entry.mechanic.fluid ??
      "",
  );
  let primaryIcon = $derived(
    kind === "recipe"
      ? "recipe"
      : kind === "mining" || kind === "generator" || kind === "boiler" || kind === "reactor"
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
          <span class="amount mono" title={`单台成本 ${solution.cost.toFixed(2)}`}>
            {solution.amount.toFixed(2)} 台 · 成本 {(solution.amount * solution.cost).toFixed(1)}
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

  .spacer {
    flex: 1;
  }
</style>
