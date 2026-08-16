<script lang="ts">
  // 单个机制卡片：主选择器（配方/资源/物品/设备）为图标按钮，
  // 机器为图标按钮，模块为小图标槽位；操作按钮为文字。
  import HoverIcon from "./HoverIcon.svelte";
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
  };

  let {
    entry,
    amount = null,
    onPick,
    onToggleEnabled,
    onRemove,
    onModuleSlot,
  }: {
    entry: MechanicEntry;
    amount?: number | null;
    onPick: (kind: CatalogKind, slot?: number) => void;
    onToggleEnabled: () => void;
    onRemove: () => void;
    onModuleSlot: (slot: number, module: string | null) => void;
  } = $props();

  let kind = $derived(entry.mechanic.type);
  let modules = $derived(entry.mechanic.module_config?.modules ?? []);
  let primaryName = $derived(
    entry.mechanic.recipe?.id ??
      entry.mechanic.item?.id ??
      entry.mechanic.seed?.id ??
      entry.mechanic.resource ??
      entry.mechanic.generator?.id ??
      entry.mechanic.boiler?.id ??
      entry.mechanic.reactor?.id ??
      "",
  );
  let primaryIcon = $derived(
    kind === "recipe"
      ? "recipe"
      : kind === "mining" || kind === "generator" || kind === "boiler" || kind === "reactor"
        ? "entity"
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
      default:
        return "item";
    }
  }

  function machineKind(): CatalogKind {
    return kind === "mining" ? "mining-machine" : "machine";
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
      />
    </button>

    <div class="main">
      <div class="name">
        {primaryName || "未设置"}
        {#if primaryName && primaryQuality !== "normal"}
          <HoverIcon type="quality" name={primaryQuality} size={14} detailKind="quality" />
        {/if}
      </div>
      <div class="meta">
        <span class="chip">{kindLabel[kind] ?? kind}</span>
        {#if amount != null}<span class="amount mono">{amount.toFixed(3)}</span>{/if}
      </div>
    </div>

    <button class="btn ghost" class:on={entry.enabled} onclick={onToggleEnabled} title={entry.enabled ? "停用" : "启用"}>
      {entry.enabled ? "启用" : "停用"}
    </button>
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
        />
      </button>
      <span class="sub">
        {machineName || "选择机器"}
        {#if machineName && machineQuality !== "normal"}
          <HoverIcon type="quality" name={machineQuality} size={14} detailKind="quality" />
        {/if}
      </span>
    {:else if kind === "generator" || kind === "boiler"}
      <button class="icon-btn" class:empty={!fluidName} title="流体" onclick={() => onPick("fluid")}>
        <HoverIcon type="fluid" name={fluidName || "fluid"} size={24} detailKind={fluidName ? "fluid" : undefined} />
      </button>
      <span class="sub">{fluidName || "选择流体"}</span>
    {:else if kind === "reactor"}
      <span class="sub">相邻 {entry.mechanic.neighbours ?? 0}</span>
    {/if}

    <span class="spacer"></span>

    {#if modules.length > 0 || kind === "recipe" || kind === "mining"}
      {#each modules as module, slot (slot)}
        <button class="icon-btn" title={`模块槽 ${slot + 1}`} onclick={() => onPick("module", slot)}>
          <HoverIcon type="item" name={module.id} size={22} detailKind="module" />
        </button>
      {/each}
      <button class="icon-btn empty" title="添加模块" onclick={() => onPick("module", modules.length)}>
        <HoverIcon type="item" name="+" size={22} />
      </button>
    {/if}
  </div>
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

  .spacer {
    flex: 1;
  }
</style>
