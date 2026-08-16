<script lang="ts">
  // 插件配置编辑器（复刻旧 egui ModuleConfigEditor）：
  // - 机器插件槽：已填槽图标（点击更换，× 移除）+ 空槽（点击添加），
  //   槽位上限来自机器原型的 module_slots（后端 ClampModules 兜底钳制）。
  // - 信标：信标实体（点击选择）、数量、共享比例、塔内插件（图标 + 数量）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import type { MechanicEntry, PrototypeDetail } from "$lib/runtime/types";

  let {
    entry,
    onPickModule,
    onPickBeacon,
    onPickBeaconModule,
  }: {
    entry: MechanicEntry;
    onPickModule: (slot: number) => void;
    onPickBeacon: (beacon: number) => void;
    onPickBeaconModule: (beacon: number, module: number) => void;
  } = $props();

  let machineDetail = $state<PrototypeDetail | null>(null);

  let modules = $derived(entry.mechanic.module_config?.modules ?? []);
  let beacons = $derived(entry.mechanic.module_config?.beacons ?? []);
  let slotCount = $derived(Math.max(machineDetail?.module_slots ?? 0, modules.length));

  // 机器变化时拉取槽位信息。
  $effect(() => {
    const machineId = entry.mechanic.machine?.id;
    const detailKind = entry.mechanic.type === "mining" ? "mining-machine" : "machine";
    if (!machineId) {
      machineDetail = null;
      return;
    }
    let alive = true;
    runtime.getDetail(detailKind, machineId).then((detail) => {
      if (alive) machineDetail = detail;
    });
    return () => {
      alive = false;
    };
  });

  function beaconCountChange(beacon: number, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(value) && value > 0) {
      runtime.moduleMessage(entry.id, { "set-beacon-count": { beacon, count: value } });
    }
  }

  function beaconShareChange(beacon: number, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(value) && value > 0) {
      runtime.moduleMessage(entry.id, { "set-beacon-share": { beacon, share: value } });
    }
  }

  function beaconModuleCountChange(beacon: number, module: number, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(value) && value >= 0) {
      runtime.moduleMessage(entry.id, {
        "set-beacon-module-count": { beacon, module, count: value },
      });
    }
  }
</script>

<div class="module-editor">
  <div class="me-label">
    插件槽（{modules.length}/{slotCount}）
    {#if slotCount > 0 && modules.length === 0}<span class="muted">点击空槽添加</span>{/if}
  </div>

  <div class="me-slots">
    {#each Array.from({ length: slotCount }) as _, i (i)}
      {#if i < modules.length}
        <div class="me-slot">
          <button class="icon-btn" title={`插件槽 ${i + 1}（点击更换）`} onclick={() => onPickModule(i)}>
            <HoverIcon type="item" name={modules[i].id} size={24} detailKind="module" />
          </button>
          <button
            class="me-x"
            title="移除插件"
            onclick={() => runtime.setModuleSlot(entry.id, i, null).catch(() => {})}
          >×</button>
        </div>
      {:else}
        <button class="icon-btn empty" title={`空插件槽 ${i + 1}`} onclick={() => onPickModule(i)}>
          <Icon type="item" name="+" size={22} />
        </button>
      {/if}
    {/each}
    {#if slotCount === 0 && modules.length > 0}
      <span class="muted">当前机器无插件槽</span>
    {/if}
  </div>

  {#if beacons.length > 0}
    <div class="me-beacons">
      <div class="me-label">信标</div>
      {#each beacons as beacon, bi (bi)}
        <div class="me-beacon">
          <div class="me-beacon-head">
            <button
              class="icon-btn"
              class:empty={!beacon.beacon.id}
              title="选择信标"
              onclick={() => onPickBeacon(bi)}
            >
              <HoverIcon
                type="entity"
                name={beacon.beacon.id || "beacon"}
                size={24}
                detailKind={beacon.beacon.id ? "beacon" : undefined}
              />
            </button>
            <label class="me-num">
              数量
              <input
                type="number"
                min="1"
                value={String(beacon.count)}
                onchange={(event) => beaconCountChange(bi, event)}
              />
            </label>
            <label class="me-num">
              共享
              <input
                type="number"
                min="0.1"
                step="0.1"
                value={String(beacon.share)}
                onchange={(event) => beaconShareChange(bi, event)}
              />
            </label>
            <button
              class="btn ghost danger"
              title="移除信标"
              onclick={() => runtime.moduleMessage(entry.id, { "remove-beacon": { beacon: bi } }).catch(() => {})}
            >×</button>
          </div>

          <div class="me-beacon-modules">
            {#each beacon.modules as [module, count], mi (mi)}
              <div class="me-beacon-module">
                <button class="icon-btn" title="选择塔内插件" onclick={() => onPickBeaconModule(bi, mi)}>
                  <HoverIcon type="item" name={module.id} size={20} detailKind="module" />
                </button>
                <input
                  type="number"
                  min="0"
                  value={String(count)}
                  onchange={(event) => beaconModuleCountChange(bi, mi, event)}
                />
                <button
                  class="btn ghost danger"
                  title="移除塔内插件"
                  onclick={() =>
                    runtime
                      .moduleMessage(entry.id, { "remove-beacon-module": { beacon: bi, module: mi } })
                      .catch(() => {})}
                >×</button>
              </div>
            {/each}
            <button
              class="btn"
              onclick={() => onPickBeaconModule(bi, beacon.modules.length)}
            >+ 塔内插件</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <button
    class="btn"
    onclick={() => runtime.moduleMessage(entry.id, "add-beacon").catch(() => {})}
  >+ 添加信标</button>
</div>

<style>
  .module-editor {
    display: grid;
    gap: 8px;
  }

  .me-label {
    color: var(--muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .me-slots {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .me-slot {
    position: relative;
    display: inline-flex;
  }

  .me-x {
    position: absolute;
    top: -5px;
    right: -5px;
    width: 14px;
    height: 14px;
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--danger);
    background: var(--danger-dim);
    border: 1px solid var(--danger-line);
    border-radius: 50%;
    font-size: 9px;
    line-height: 1;
    cursor: pointer;
  }

  .me-beacons {
    display: grid;
    gap: 8px;
  }

  .me-beacon {
    display: grid;
    gap: 6px;
    padding: 8px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .me-beacon-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .me-num {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: 10px;
  }

  .me-num input {
    width: 52px;
    min-height: 22px;
    padding: 0 4px;
    text-align: right;
    background: var(--card);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }

  .me-beacon-modules {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    padding-left: 2px;
  }

  .me-beacon-module {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 2px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .me-beacon-module input {
    width: 40px;
    min-height: 20px;
    padding: 0 4px;
    text-align: right;
    background: var(--bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }
</style>
