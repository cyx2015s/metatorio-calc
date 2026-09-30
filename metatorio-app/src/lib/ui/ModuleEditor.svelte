<script lang="ts">
  // 插件配置编辑器（复刻旧 egui ModuleConfigEditor）：
  // - 机器插件槽：已填槽图标（点击更换，× 移除）+ 空槽（点击添加），
  //   槽位上限来自机器原型的 module_slots（后端 ClampModules 兜底钳制）。
  // - 插件塔：插件塔实体（点击选择）、数量、共享比例、塔内插件（图标 + 数量）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import type { MechanicEntry, PrototypeDetail } from "$lib/runtime/types";

  let {
    entry,
    onPickModule,
    onPickBeacon,
    onPickBeaconModule,
    onAddBeacon,
  }: {
    entry: MechanicEntry;
    onPickModule: (slot: number) => void;
    onPickBeacon: (beacon: number) => void;
    onPickBeaconModule: (beacon: number, module: number) => void;
    /** 直接选一个插件塔添加（插件塔配置必须绑定有效插件塔，不允许空配置行）。 */
    onAddBeacon: () => void;
  } = $props();

  let machineDetail = $state<PrototypeDetail | null>(null);
  /** #15：插件塔数量/插件数量校验失败时的可见提示（原先只有 console.warn，用户看不到）。 */
  let beaconWarning = $state<string | null>(null);

  let modules = $derived(entry.mechanic.module_config?.modules ?? []);
  let beacons = $derived(entry.mechanic.module_config?.beacons ?? []);
  let slotCount = $derived(Math.max(machineDetail?.module_slots ?? 0, modules.length));
  /** 机器是否吃插件塔效果（EffectReceiver.uses_beacon_effects，未声明按 true）。
   *  false 时不允许添加插件塔——加了也不生效（后端同样忽略其效果与耗电）。 */
  let usesBeaconEffects = $derived(machineDetail?.uses_beacon_effects ?? true);

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
    const input = event.currentTarget as HTMLInputElement;
    const value = Number(input.value);
    if (Number.isFinite(value) && value > 0) {
      beaconWarning = null;
      runtime.moduleMessage(entry.id, { "set-beacon-count": { beacon, count: value } });
    } else {
      // 拒绝写入时把输入框还原，避免界面上的数字与文档不一致。
      input.value = String(beacons[beacon]?.count ?? 1);
      beaconWarning = "插件塔座数必须是大于 0 的整数";
    }
  }

  function beaconShareChange(beacon: number, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(value) && value > 0) {
      runtime.moduleMessage(entry.id, { "set-beacon-share": { beacon, share: value } });
    }
  }

  function beaconModuleCountChange(beacon: number, module: number, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const value = Number(input.value);
    const beaconCfg = beacons[beacon];
    if (!beaconCfg) return;
    const current = beaconCfg.modules[module]?.[1] ?? 0;
    if (!Number.isFinite(value) || value < 0) {
      input.value = String(current);
      beaconWarning = "插件数量必须是不小于 0 的整数";
      return;
    }
    void (async () => {
      const perBeacon = await beaconSlotsOf(beaconCfg.beacon.id);
      const slots = perBeacon * beaconCfg.count;
      if (slots > 0) {
        const total =
          beaconCfg.modules.reduce((sum, [, count], index) => {
            return sum + (index === module ? 0 : count);
          }, 0) + value;
        if (total > slots) {
          input.value = String(current);
          beaconWarning = `插件数量超出槽位上限：${beaconCfg.count} 座塔 × 每塔 ${perBeacon} 槽 = ${slots}`;
          return;
        }
      }
      beaconWarning = null;
      runtime.moduleMessage(entry.id, {
        "set-beacon-module-count": { beacon, module, count: value },
      });
    })();
  }

  /** 插件塔原型插件槽数（getDetail 异步；未知默认 2）。 */
  async function beaconSlotsOf(beaconId: string): Promise<number> {
    const detail = await runtime.getDetail("beacon", beaconId);
    return detail?.beacon_module_slots ?? detail?.module_slots ?? 2;
  }
</script>

<div class="module-editor">
  <div class="me-slots-row">
    <span class="me-label">
      插件槽（{modules.length}/{slotCount}）
      {#if slotCount > 0 && modules.length === 0}<span class="muted">点击空槽添加</span>{/if}
    </span>
    <div class="me-slots">
      {#each Array.from({ length: slotCount }) as _, i (i)}
        {#if i < modules.length}
          <div class="me-slot">
            <button class="icon-btn" title={`插件槽 ${i + 1}（点击更换）`} onclick={() => onPickModule(i)}>
              <HoverIcon type="item" name={modules[i].id} size={24} detailKind="module" quality={modules[i].quality} />
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
  </div>

  {#if beacons.length > 0}
    <div class="me-beacons">
      <div class="me-beacons-head">
        <span class="me-label">插件塔</span>
        {#if usesBeaconEffects}
          <button
            class="btn"
            title="选择插件塔添加到这台机器"
            onclick={onAddBeacon}
          >+ 添加插件塔</button>
        {:else}
          <span class="muted">该机器不受插件塔影响</span>
        {/if}
      </div>
      {#if beaconWarning}
        <div class="me-warn">{beaconWarning}</div>
      {/if}
      <div class="me-hint">
        塔内「插件数量」= 全部插件塔加起来的总数（例：10 座塔 × 每塔 2 槽 → 填 20）
      </div>
      {#each beacons as beacon, bi (bi)}
        <div class="me-beacon">
          <div class="me-beacon-head">
            <button
              class="icon-btn"
              class:empty={!beacon.beacon.id}
              title="选择插件塔"
              onclick={() => onPickBeacon(bi)}
            >
              <HoverIcon
                type="entity"
                name={beacon.beacon.id || "beacon"}
                size={24}
                detailKind={beacon.beacon.id ? "beacon" : undefined}
                quality={beacon.beacon.quality}
              />
            </button>
            <label class="me-num" title="插件塔的座数（不是塔内插件数）：塔内插件上限 = 塔数 × 每塔插件槽数">
              塔数
              <input
                type="number"
                min="1"
                value={String(beacon.count)}
                title="插件塔的座数（不是塔内插件数）"
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
              title="移除插件塔"
              onclick={() => runtime.moduleMessage(entry.id, { "remove-beacon": { beacon: bi } }).catch(() => {})}
            >×</button>
          </div>

          <div class="me-beacon-modules">
            {#each beacon.modules as [module, count], mi (mi)}
              <div class="me-beacon-module">
                <button class="icon-btn" title="选择塔内插件" onclick={() => onPickBeaconModule(bi, mi)}>
                  <HoverIcon type="item" name={module.id} size={20} detailKind="module" quality={module.quality} />
                </button>
                <input
                  type="number"
                  min="0"
                  value={String(count)}
                  title="该插件在全部插件塔中的总数；上限 = 塔数 × 每塔插件槽数"
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
  {:else if usesBeaconEffects}
    <button
      class="btn"
      title="选择插件塔添加到这台机器"
      onclick={onAddBeacon}
    >+ 添加插件塔</button>
  {/if}
</div>

<style>
  .module-editor {
    display: grid;
    gap: 5px;
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

  .me-slots-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .me-slots-row .me-label {
    flex: 0 0 auto;
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
    gap: 5px;
  }

  .me-beacons-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .me-beacons-head .me-label {
    flex: 1;
  }

  /* #15：插件数量口径说明与校验失败提示 */
  .me-hint {
    color: var(--muted);
    font-size: 10px;
    line-height: 1.4;
  }

  .me-warn {
    color: var(--danger);
    font-size: 10px;
    line-height: 1.4;
  }

  .me-beacon {
    display: grid;
    gap: 5px;
    padding: 6px;
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
