<script module lang="ts">
  import type { ModuleConfig } from "$lib/runtime/types";

  // 模块级共享内存剪贴板：在任意机制卡之间一键复制/粘贴插件与插件塔配置。
  let copiedConfig = $state<ModuleConfig | null>(null);
</script>

<script lang="ts">
  // 插件配置编辑器（复刻旧 egui ModuleConfigEditor）：
  // - 机器插件槽：已填槽图标（点击更换，右键 / × 移除）+ 空槽（点击添加），
  //   槽位上限来自机器原型的 module_slots（后端 ClampModules 兜底钳制）。
  // - 插件塔：插件塔实体（点击选择）、数量、共享比例、塔内插件（图标 + 数量）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import HoverIcon from "./HoverIcon.svelte";
  import Icon from "./Icon.svelte";
  import type { MechanicActions } from "$lib/mechanics/actions";
  import type { MechanicEntry, PrototypeDetail } from "$lib/runtime/types";

  let {
    entry,
    actions,
    onPickModule,
    onPickBeacon,
    onPickBeaconModule,
    onAddBeacon,
  }: {
    entry: MechanicEntry;
    actions: MechanicActions;
    onPickModule: (slot: number) => void;
    onPickBeacon: (beacon: number) => void;
    onPickBeaconModule: (beacon: number, module: number) => void;
    /** 直接选一个插件塔添加（插件塔配置必须绑定有效插件塔，不允许空配置行）。 */
    onAddBeacon: () => void;
  } = $props();

  let machineDetail = $state<PrototypeDetail | null>(null);
  /** #15：插件塔数量/插件数量校验失败时的可见提示（原先只有 console.warn，用户看不到）。 */
  let beaconWarning = $state<string | null>(null);
  let batchBusy = $state(false);
  let batchNotice = $state<string | null>(null);

  let modules = $derived(entry.mechanic.module_config?.modules ?? []);
  let beacons = $derived(entry.mechanic.module_config?.beacons ?? []);
  let slotCount = $derived(Math.max(machineDetail?.module_slots ?? 0, modules.length));
  /** 机器是否吃插件塔效果（EffectReceiver.uses_beacon_effects，未声明按 true）。
   *  false 时不允许添加插件塔——加了也不生效（后端同样忽略其效果与耗电）。 */
  let usesBeaconEffects = $derived(machineDetail?.uses_beacon_effects ?? true);

  // 机器变化时拉取槽位信息。
  $effect(() => {
    void entry.id;
    batchNotice = null;
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

  async function fillRemainingSlots() {
    if (batchBusy || modules.length === 0 || modules.length >= slotCount) return;
    const last = modules[modules.length - 1];
    batchBusy = true;
    batchNotice = null;
    try {
      for (let slot = modules.length; slot < slotCount; slot++) {
        await actions.commands.setModuleSlot(entry.id, slot, last.id, last.quality);
      }
    } catch {
      batchNotice = "填满剩余插件槽失败";
    } finally {
      batchBusy = false;
    }
  }

  async function clearAllModules() {
    if (batchBusy || modules.length === 0) return;
    batchBusy = true;
    batchNotice = null;
    try {
      await actions.commands.clearModules(entry.id);
    } catch {
      batchNotice = "清空插件失败";
    } finally {
      batchBusy = false;
    }
  }

  function copyModuleConfig() {
    copiedConfig = JSON.parse(
      JSON.stringify({
        modules: entry.mechanic.module_config?.modules ?? [],
        beacons: entry.mechanic.module_config?.beacons ?? [],
      }),
    ) as ModuleConfig;
    batchNotice = "已复制整套插件配置";
  }

  async function pasteModuleConfig() {
    if (batchBusy || !copiedConfig) return;
    batchBusy = true;
    batchNotice = null;
    beaconWarning = null;
    try {
      const snapshot = JSON.parse(JSON.stringify(copiedConfig)) as ModuleConfig;
      const machineId = entry.mechanic.machine?.id;
      const detailKind = entry.mechanic.type === "mining" ? "mining-machine" : "machine";
      const detail = machineDetail ?? (machineId ? await runtime.getDetail(detailKind, machineId) : null);
      const maxSlots = detail?.module_slots ?? 0;
      const allowBeacons = detail?.uses_beacon_effects ?? true;
      const targetModules = snapshot.modules.slice(0, maxSlots);
      const truncated = snapshot.modules.length > maxSlots;

      if (modules.length > 0) {
        await actions.commands.clearModules(entry.id);
      }
      for (let slot = 0; slot < targetModules.length; slot++) {
        const mod = targetModules[slot];
        await actions.commands.setModuleSlot(entry.id, slot, mod.id, mod.quality);
      }

      let skippedBeacons = false;
      if (allowBeacons) {
        for (let bi = beacons.length - 1; bi >= 0; bi--) {
          await actions.commands.moduleMessage(entry.id, { "remove-beacon": { beacon: bi } });
        }
        for (let bi = 0; bi < snapshot.beacons.length; bi++) {
          const beaconCfg = snapshot.beacons[bi];
          await actions.commands.moduleMessage(entry.id, {
            "add-beacon": { beacon: { id: beaconCfg.beacon.id, quality: beaconCfg.beacon.quality } },
          });
          await actions.commands.moduleMessage(entry.id, {
            "set-beacon-count": { beacon: bi, count: beaconCfg.count },
          });
          await actions.commands.moduleMessage(entry.id, {
            "set-beacon-share": { beacon: bi, share: beaconCfg.share },
          });
          for (let mi = 0; mi < beaconCfg.modules.length; mi++) {
            const [mod, count] = beaconCfg.modules[mi];
            await actions.commands.moduleMessage(entry.id, {
              "add-beacon-module": { beacon: bi, module: { id: mod.id, quality: mod.quality } },
            });
            await actions.commands.moduleMessage(entry.id, {
              "set-beacon-module-count": { beacon: bi, module: mi, count },
            });
          }
        }
      } else if (snapshot.beacons.length > 0) {
        skippedBeacons = true;
      }

      const notes: string[] = [];
      if (truncated) notes.push(`机器插件已按 ${maxSlots} 槽上限截断`);
      if (skippedBeacons) notes.push("当前机器不受插件塔影响，已跳过插件塔");
      batchNotice = notes.length > 0 ? `已粘贴配置（${notes.join("；")}）` : "已粘贴插件配置";
    } catch {
      batchNotice = "粘贴配置失败";
    } finally {
      batchBusy = false;
    }
  }

  function beaconCountChange(beacon: number, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const value = Number(input.value);
    if (Number.isFinite(value) && value > 0) {
      beaconWarning = null;
      actions.commands.moduleMessage(entry.id, { "set-beacon-count": { beacon, count: value } });
    } else {
      // 拒绝写入时把输入框还原，避免界面上的数字与文档不一致。
      input.value = String(beacons[beacon]?.count ?? 1);
      beaconWarning = "插件塔座数必须是大于 0 的整数";
    }
  }

  function beaconShareChange(beacon: number, event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(value) && value > 0) {
      actions.commands.moduleMessage(entry.id, { "set-beacon-share": { beacon, share: value } });
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
      actions.commands.moduleMessage(entry.id, {
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
            <button
              class="icon-btn"
              aria-label={`插件槽 ${i + 1}（点击更换，右键移除）`}
              onclick={() => onPickModule(i)}
              oncontextmenu={(event) => {
                event.preventDefault();
                actions.commands.setModuleSlot(entry.id, i, null).catch(() => {});
              }}
            >
              <HoverIcon type="item" name={modules[i].id} size={24} detailKind="module" quality={modules[i].quality} hint="点击更换 · 右键移除" />
            </button>
            <button
              class="me-x"
              aria-label={`移除插件槽 ${i + 1}`}
              title="移除插件"
              onclick={() => actions.commands.setModuleSlot(entry.id, i, null).catch(() => {})}
            >×</button>
          </div>
        {:else}
          <button class="icon-btn empty" title={`空插件槽 ${i + 1}`} aria-label={`空插件槽 ${i + 1}`} onclick={() => onPickModule(i)}>
            <Icon type="item" name="+" size={22} title={`空插件槽 ${i + 1}`} />
          </button>
        {/if}
      {/each}
      {#if slotCount === 0 && modules.length > 0}
        <span class="muted">当前机器无插件槽</span>
      {/if}
    </div>
    <div class="me-slot-actions">
      {#if slotCount > 0}
        <button
          type="button"
          class="btn me-batch-btn"
          disabled={batchBusy || modules.length === 0 || modules.length >= slotCount}
          title="用当前最后一个插件填满所有空槽"
          onclick={fillRemainingSlots}
        >填满剩余槽</button>
        <button
          type="button"
          class="btn ghost me-batch-btn"
          disabled={batchBusy || modules.length === 0}
          title="清空全部机器插件"
          onclick={clearAllModules}
        >清空</button>
      {/if}
      <button
        type="button"
        class="btn ghost me-batch-btn"
        disabled={batchBusy || (modules.length === 0 && beacons.length === 0)}
        title="复制当前机器插件与插件塔配置"
        onclick={copyModuleConfig}
      >复制配置</button>
      <button
        type="button"
        class="btn ghost me-batch-btn"
        disabled={batchBusy || !copiedConfig}
        title="粘贴已复制的插件与插件塔配置（按槽位上限截断）"
        onclick={pasteModuleConfig}
      >粘贴配置</button>
    </div>
    {#if batchNotice}
      <div class="me-batch-note" role="status">{batchNotice}</div>
    {/if}
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
              aria-label="选择插件塔"
              onclick={() => onPickBeacon(bi)}
            >
              <HoverIcon
                type="entity"
                name={beacon.beacon.id || "beacon"}
                size={24}
                detailKind={beacon.beacon.id ? "beacon" : undefined}
                quality={beacon.beacon.quality}
                hint={beacon.beacon.id ? "点击更换插件塔" : undefined}
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
            <label class="me-num" title="每座插件塔平均覆盖的机器数；1 表示不共享。只用于分摊耗电和面积，不改变插件效果">
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
              onclick={() => actions.commands.moduleMessage(entry.id, { "remove-beacon": { beacon: bi } }).catch(() => {})}
            >×</button>
          </div>

          <div class="me-beacon-modules">
            {#each beacon.modules as [module, count], mi (mi)}
              <div class="me-beacon-module">
                <button class="icon-btn" aria-label="选择塔内插件" onclick={() => onPickBeaconModule(bi, mi)}>
                  <HoverIcon type="item" name={module.id} size={20} detailKind="module" quality={module.quality} hint="点击更换塔内插件" />
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
                    actions.commands
                      .moduleMessage(entry.id, { "remove-beacon-module": { beacon: bi, module: mi } })
                      .catch(() => {})}
                >×</button>
              </div>
            {/each}
            <button
              class="btn"
              title="向这座插件塔添加一种插件"
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
    gap: 7px;
  }

  .me-label {
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .me-slots {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    align-items: center;
  }

  .me-slots-row {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
  }

  .me-slot-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }

  .me-batch-btn {
    min-height: 24px;
    padding: 0 7px;
    font-size: 10.5px;
  }

  .me-batch-note {
    color: var(--muted);
    font-size: 10.5px;
    line-height: 1.4;
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
    top: -6px;
    right: -6px;
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--danger);
    background: var(--danger-dim);
    border: 1px solid var(--danger-line);
    border-radius: 50%;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
    transition: background-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1)),
      border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .me-x:hover {
    color: var(--accent-ink);
    background: var(--danger);
    border-color: var(--danger);
  }

  .me-beacons {
    display: grid;
    gap: 7px;
  }

  .me-beacons-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .me-beacons-head .me-label {
    flex: 1;
  }

  .me-hint {
    color: var(--faint);
    font-size: 9.5px;
    line-height: 1.5;
  }

  .me-warn {
    padding: 6px 8px;
    color: var(--danger);
    background: var(--danger-dim);
    border: 1px solid var(--danger-line);
    border-radius: var(--radius-sm);
    font-size: 10.5px;
    line-height: 1.45;
  }

  .me-beacon {
    display: grid;
    gap: 6px;
    padding: 8px;
    background: var(--panel-elev);
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
    min-height: 24px;
    padding: 0 5px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 10px;
    text-align: right;
  }

  .me-num input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

  .me-beacon-modules {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    padding-left: 2px;
  }

  .me-beacon-module {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 2px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    transition: border-color var(--transition-fast, 120ms cubic-bezier(0.22, 1, 0.36, 1));
  }

  .me-beacon-module:hover {
    border-color: var(--line-strong);
  }

  .me-beacon-module input {
    width: 42px;
    min-height: 22px;
    padding: 0 4px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: 3px;
    font-family: var(--mono);
    font-variant-numeric: tabular-nums;
    font-size: 10px;
    text-align: right;
  }

  .me-beacon-module input:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
  }

</style>
