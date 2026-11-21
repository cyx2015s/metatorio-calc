<script module lang="ts">
  import type { CatalogKind } from "$lib/runtime/types";

  export type SelectorRequest = {
    kind: CatalogKind;
    title: string;
    kinds: { kind: CatalogKind; label: string }[];
    categoryFilter?: string[];
    tilePurpose?: "extract" | "dispose";
    allowedNames?: string[];
    initialName?: string;
    initialQuality?: string;
    onSelect: (name: string, quality: string) => void;
    onPickKind?: (kind: CatalogKind, name: string, quality: string) => void;
    respectAccessibility?: boolean;
    multiLevelTechOnly?: boolean;
  };

  export type OpenSelectorFn = (
    request: Omit<SelectorRequest, "kinds"> & { kinds?: { kind: CatalogKind; label: string }[] },
  ) => void;
</script>

<script lang="ts">
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import type { ProjectDocument } from "$lib/runtime/types";
  import HoverIcon from "$lib/ui/HoverIcon.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import RichText from "$lib/ui/RichText.svelte";
  import { createBackdropClose } from "$lib/ui/modal-backdrop";

  let {
    prefsOpen = $bindable(false),
    project,
    onOpenSelector,
    onNotice,
  }: {
    prefsOpen: boolean;
    project: ProjectDocument | null;
    onOpenSelector: OpenSelectorFn;
    onNotice: (message: string) => void;
  } = $props();

  let planning = $derived(project?.planning ?? null);
  const prefsBackdrop = createBackdropClose(() => (prefsOpen = false));

  /** 插件塔原型的插件槽数（getDetail 异步；未知时默认 2，与 Factorio 一致）。 */
  async function beaconModuleSlots(beaconId: string | undefined): Promise<number> {
    if (!beaconId) return 2;
    const detail = await runtime.getDetail("beacon", beaconId);
    return detail?.beacon_module_slots ?? detail?.module_slots ?? 2;
  }
</script>

{#if prefsOpen && planning}
  <!-- z-index 低于选择器（40），从弹窗内打开选择器时选择器叠在上层 -->
  <div class="backdrop low" role="button" tabindex="0" {...prefsBackdrop}>
    <div class="prefs-modal">
      <div class="mini-title">规划偏好</div>

      <div class="field">
        <label for="alt-count">替代数量（自动规划时每个配方枚举几个备选机器）</label>
        <input
          id="alt-count"
          type="number"
          min="1"
          max="3"
          value={String(planning.alternative_count)}
          onchange={(event) => {
            const value = Number((event.currentTarget as HTMLInputElement).value);
            if (Number.isFinite(value) && value >= 1 && value <= 3) {
              runtime.setAlternativeCount(value).catch(() => {});
            }
          }}
        />
      </div>

      <div class="prefs-section">
        <div class="prefs-title">机器偏好（自动选机时优先）</div>
        <div class="prefs-list">
          {#each planning.machine_preferences as pref, i (i)}
            <div class="prefs-item">
              <HoverIcon type="entity" name={pref.id} size={22} detailKind="machine" quality={pref.quality} />
              <span class="prefs-name"><RichText text={runtime.richName("machine", pref.id)} /></span>
              <button
                class="btn ghost"
                title="移除"
                onclick={() => runtime.removeMachinePreference(pref).catch(() => {})}
              >×</button>
            </div>
          {:else}
            <span class="muted">还没有机器偏好</span>
          {/each}
        </div>
        <button
          class="btn block"
          onclick={() =>
            onOpenSelector({
              kind: "machine",
              title: "添加机器偏好",
              onSelect: (name, quality) => runtime.addMachinePreference({ id: name, quality }),
            })}
        >+ 添加机器</button>
      </div>

      <div class="prefs-section">
        <div class="prefs-title">枚举插件（自动规划参与组合的插件）</div>
        <div class="prefs-list">
          {#each planning.enumerate_modules as module, i (i)}
            <div class="prefs-item">
              <HoverIcon type="item" name={module.id} size={22} detailKind="module" quality={module.quality} />
              <span class="prefs-name"><RichText text={runtime.richName("module", module.id)} /></span>
              <button
                class="btn ghost"
                title="移除"
                onclick={() => runtime.removeEnumeratedModule(module).catch(() => {})}
              >×</button>
            </div>
          {:else}
            <span class="muted">还没有枚举插件</span>
          {/each}
        </div>
        <button
          class="btn block"
          onclick={() =>
            onOpenSelector({
              kind: "module",
              title: "添加枚举插件",
              onSelect: (name, quality) => runtime.addEnumeratedModule(name, quality),
            })}
        >+ 添加插件</button>
        <button
          class="btn block"
          title="用每插件类别中最高 tier 的插件（工厂主品质）替换枚举列表"
          onclick={() =>
            runtime
              .applyBestModules(project?.settings.major_quality || "normal")
              .catch(() => {})}
        >使用最佳插件</button>
      </div>

      <div class="prefs-section">
        <div class="prefs-title">枚举插件塔（自动规划叠加的插件塔方案）</div>
        <div class="prefs-list">
          {#each planning.enumerate_beacons as plan, i (i)}
            {@const beacon = plan.module_config?.beacons?.[0]?.beacon}
            {@const beaconConfig = plan.module_config?.beacons?.[0]}
            <div class="prefs-item column">
              <div class="prefs-row">
                <button
                  class="icon-btn"
                  title="选择插件塔"
                  onclick={() =>
                    onOpenSelector({
                      kind: "beacon",
                      title: "选择枚举插件塔",
                      onSelect: (name, quality) =>
                        runtime
                          .enumeratedBeaconModule(i, {
                            "add-beacon": { beacon: { id: name, quality } },
                          })
                          .catch(() => {}),
                    })}
                >
                  {#if beacon}
                    <HoverIcon type="entity" name={beacon.id} size={24} detailKind="beacon" quality={beacon.quality} />
                  {:else}
                    <Icon type="entity" name="beacon" size={24} title="选择插件塔" />
                  {/if}
                </button>
                <span class="prefs-name">{beacon ? runtime.localizedName("beacon", beacon.id) : "未选插件塔"}</span>
                <button
                  class="btn ghost danger"
                  title="移除"
                  onclick={() => runtime.removeEnumeratedBeacon(i).catch(() => {})}
                >×</button>
              </div>
              {#if beaconConfig}
                <div class="prefs-row sub">
                  <label class="me-num" title="插件塔的座数（不是塔内插件数）：塔内插件上限 = 塔数 × 每塔插件槽数">
                    塔数
                    <input
                      type="number"
                      min="1"
                      value={String(beaconConfig.count)}
                      title="插件塔的座数（不是塔内插件数）"
                      onchange={(event) => {
                        const input = event.currentTarget as HTMLInputElement;
                        const value = Number(input.value);
                        if (Number.isFinite(value) && value > 0) {
                          runtime
                            .enumeratedBeaconModule(i, { "set-beacon-count": { beacon: 0, count: value } })
                            .catch(() => {});
                        } else {
                          input.value = String(beaconConfig.count);
                          onNotice("插件塔座数必须是大于 0 的整数");
                        }
                      }}
                    />
                  </label>
                  <label class="me-num">
                    共享
                    <input
                      type="number"
                      min="0.1"
                      step="0.1"
                      value={String(beaconConfig.share)}
                      onchange={(event) => {
                        const value = Number((event.currentTarget as HTMLInputElement).value);
                        if (Number.isFinite(value) && value > 0) {
                          runtime
                            .enumeratedBeaconModule(i, { "set-beacon-share": { beacon: 0, share: value } })
                            .catch(() => {});
                        }
                      }}
                    />
                  </label>
                  <button
                    class="btn"
                    title="添加塔内插件"
                    onclick={() =>
                      onOpenSelector({
                        kind: "module",
                        title: "选择塔内插件",
                        onSelect: (name, quality) =>
                          runtime
                            .enumeratedBeaconModule(i, {
                              "add-beacon-module": {
                                beacon: 0,
                                module: { id: name, quality },
                              },
                            })
                            .catch(() => {}),
                      })}
                  >+ 插件</button>
                </div>
                {#if beaconConfig.modules.length > 0}
                  <div class="prefs-hint">
                    插件数量 = 全部插件塔加起来的总数（例：10 座塔 × 每塔 2 槽 → 填 20），不是每座塔的插件数
                  </div>
                  <div class="prefs-row wrap">
                    {#each beaconConfig.modules as [module, count], mi (mi)}
                      <span class="prefs-chip">
                        <HoverIcon type="item" name={module.id} size={16} detailKind="module" quality={module.quality} />
                        <input
                          class="chip-num"
                          type="number"
                          min="1"
                          value={String(count)}
                          title="该插件在全部插件塔中的总数；上限 = 塔数 × 每塔插件槽数"
                          onchange={(event) => {
                            const input = event.currentTarget as HTMLInputElement;
                            const value = Number(input.value);
                            if (!Number.isFinite(value) || value < 1) {
                              input.value = String(count);
                              onNotice("插件数量必须是大于 0 的整数");
                              return;
                            }
                            void (async () => {
                              const perBeacon = await beaconModuleSlots(beacon?.id);
                              const slots = perBeacon * beaconConfig.count;
                              const total =
                                beaconConfig.modules.reduce(
                                  (sum, [, c], index) => sum + (index === mi ? 0 : c),
                                  0,
                                ) + value;
                              if (slots > 0 && total > slots) {
                                input.value = String(count);
                                onNotice(
                                  `插件数量超出槽位上限：${beaconConfig.count} 座塔 × 每塔 ${perBeacon} 槽 = ${slots}`,
                                );
                                return;
                              }
                              runtime
                                .enumeratedBeaconModule(i, {
                                  "set-beacon-module-count": { beacon: 0, module: mi, count: value },
                                })
                                .catch(() => {});
                            })();
                          }}
                        />
                        <button
                          class="prefs-chip-x"
                          title="移除塔内插件"
                          onclick={() =>
                            runtime
                              .enumeratedBeaconModule(i, { "remove-beacon-module": { beacon: 0, module: mi } })
                              .catch(() => {})}
                        >×</button>
                      </span>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>
          {:else}
            <span class="muted">还没有枚举插件塔</span>
          {/each}
        </div>
        <button
          class="btn block"
          onclick={() =>
            onOpenSelector({
              kind: "beacon",
              title: "添加枚举插件塔",
              onSelect: (name, quality) => runtime.addEnumeratedBeacon({ id: name, quality }),
            })}
        >+ 添加插件塔</button>
      </div>

      <div class="mini-actions">
        <button class="btn primary" onclick={() => (prefsOpen = false)}>完成</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    z-index: 50;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--backdrop);
    backdrop-filter: blur(6px);
  }

  .backdrop.low {
    z-index: 30;
  }

  .prefs-modal {
    width: min(460px, 100%);
    max-height: min(720px, calc(100vh - 48px));
    display: grid;
    gap: 14px;
    padding: 18px;
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-strong);
  }

  .prefs-section {
    display: grid;
    gap: 8px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }

  .mini-title {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line);
    font-size: 13.5px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .mini-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    padding-top: 12px;
    margin-top: 2px;
    border-top: 1px solid var(--line);
  }

  .prefs-list {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .prefs-item {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }

  .prefs-item:hover {
    background: var(--card-hover);
    border-color: var(--line);
  }

  .prefs-item.column {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    padding: 8px;
    border-color: var(--line);
    background: var(--input-bg);
  }

  .prefs-name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .prefs-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .prefs-row.sub,
  .prefs-row.wrap {
    flex-wrap: wrap;
  }

  .prefs-title {
    margin: 6px 0 2px;
    color: var(--faint);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .prefs-hint {
    color: var(--faint);
    font-size: 9.5px;
    line-height: 1.45;
  }

  .prefs-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 5px 1px 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10px;
  }

  .prefs-chip-x {
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    padding: 0;
    color: var(--danger);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }

  .me-num input,
  .chip-num {
    min-height: 24px;
    padding: 0 5px;
    color: var(--text);
    background: var(--input-bg);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
    text-align: right;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .me-num input:focus,
  .chip-num:focus {
    border-color: var(--accent);
    outline: none;
    box-shadow: 0 0 0 1px var(--accent);
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
  }

  .chip-num {
    width: 42px;
  }
</style>
