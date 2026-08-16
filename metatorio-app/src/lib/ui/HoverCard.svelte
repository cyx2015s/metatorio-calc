<script lang="ts">
  // 悬停信息卡片：显示原型名称、类型与关键参数（数据来自后端 prototype_detail，
  // 按 (kind, name) 缓存）。配方为核心内容，参考原 egui PrototypeHover：
  // 时间/类别/表面条件/产能上限 + 原料/产物（概率折算、产能额外、品质范围偏移、
  // 流体温度），一次尽量显示全。卡片超出视口时整体平移回屏。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import Icon from "./Icon.svelte";
  import type { FlowAmount, PrototypeDetail } from "$lib/runtime/types";

  let {
    kind,
    detail,
    x,
    y,
  }: { kind: string; detail: PrototypeDetail | null; x: number; y: number } = $props();

  const kindLabel: Record<string, string> = {
    item: "物品",
    fluid: "流体",
    recipe: "配方",
    module: "模块",
    machine: "制造机",
    "mining-machine": "采矿机",
    generator: "发电机",
    boiler: "锅炉",
    reactor: "反应堆",
    beacon: "信标",
    resource: "资源",
    entity: "实体",
    technology: "科技",
    planet: "星球",
    surface: "地表",
    quality: "品质",
  };

  function iconTypeOf(kind: string): string {
    if (kind === "recipe") return "recipe";
    if (kind === "fluid") return "fluid";
    if (kind === "item" || kind === "module") return "item";
    if (kind === "quality") return "quality";
    return "entity";
  }

  function formatPower(joulesPerTick: number): string {
    const watts = joulesPerTick * 60;
    if (watts >= 1e6) return `${(watts / 1e6).toFixed(2)} MW`;
    if (watts >= 1e3) return `${(watts / 1e3).toFixed(1)} kW`;
    return `${watts.toFixed(0)} W`;
  }

  function formatMultiplier(value: number): string {
    return `×${value.toFixed(2)}`;
  }

  /** 数字显示：整数不带小数，小数额外保留 3 位去尾零。 */
  function fmtNum(value: number): string {
    if (Number.isInteger(value)) return String(value);
    return value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
  }

  function flowName(flow: FlowAmount): string {
    return runtime.localizedName(flow.kind, flow.name);
  }

  function qualityLabel(name: string | null): string {
    return name ? runtime.localizedName("quality", name) : "";
  }

  /** 原料/产物的元信息片段：期望量、概率、产能额外、品质范围/偏移、温度。 */
  function flowMeta(flow: FlowAmount): string[] {
    const parts: string[] = [fmtNum(flow.amount)];
    if (flow.probability < 1) parts.push(`${(flow.probability * 100).toFixed(1)}%`);
    if (flow.productivity > 0) parts.push(`+${fmtNum(flow.productivity)}/产能`);
    const qmin = qualityLabel(flow.quality_min);
    const qmax = qualityLabel(flow.quality_max);
    if (qmin || qmax) {
      let quality = qmin ? (qmax && qmin !== qmax ? `${qmin}~${qmax}` : qmin) : (qmax || "");
      if (flow.quality_change) {
        quality += ` 偏移${flow.quality_change > 0 ? "+" : ""}${flow.quality_change}`;
      }
      parts.push(`品质 ${quality}`);
    }
    if (flow.temperature != null) {
      parts.push(`@${fmtNum(flow.temperature)}℃`);
    }
    if (flow.min_temperature != null && flow.max_temperature != null) {
      parts.push(`${fmtNum(flow.min_temperature)}~${fmtNum(flow.max_temperature)}℃`);
    } else if (flow.min_temperature != null) {
      parts.push(`≥${fmtNum(flow.min_temperature)}℃`);
    } else if (flow.max_temperature != null) {
      parts.push(`≤${fmtNum(flow.max_temperature)}℃`);
    }
    return parts;
  }

  // 溢出修正：卡片渲染后若超出视口右下，整体平移回屏。
  let cardEl = $state<HTMLDivElement | null>(null);
  let shift = $state({ dx: 0, dy: 0 });
  $effect(() => {
    const el = cardEl;
    if (!el) return;
    void x;
    void y;
    const rect = el.getBoundingClientRect();
    const dx = Math.min(0, window.innerWidth - 8 - rect.right);
    const dy = Math.min(0, window.innerHeight - 8 - rect.bottom);
    if (dx !== shift.dx || dy !== shift.dy) shift = { dx, dy };
  });
</script>

{#if detail}
  <div
    bind:this={cardEl}
    class="hover-card"
    style={`left:${x + 14 + shift.dx}px;top:${y + 14 + shift.dy}px`}
  >
    <div class="hc-head">
      <Icon type={iconTypeOf(detail.kind)} name={detail.name} size={32} />
      <div class="hc-title">
        <strong>{detail.localized_name || detail.name}</strong>
        {#if detail.localized_name}<small>{detail.name}</small>{/if}
        <small>{kindLabel[kind] ?? kind}</small>
      </div>
    </div>
    <div class="hc-body">
      {#if detail.stack_size != null}
        <div class="hc-row"><span>堆叠</span><strong>{detail.stack_size}</strong></div>
      {/if}
      {#if detail.category}
        <div class="hc-row"><span>类别</span><strong>{detail.category}</strong></div>
      {/if}
      {#if detail.energy_required != null}
        <div class="hc-row"><span>时间</span><strong>{detail.energy_required}s</strong></div>
      {/if}
      {#if detail.maximum_productivity != null && detail.maximum_productivity !== 3}
        <div class="hc-row"><span>产能上限</span><strong>×{fmtNum(detail.maximum_productivity)}</strong></div>
      {/if}
      {#if detail.surface_conditions.length > 0}
        <div class="hc-row"><span>表面条件</span><strong>{detail.surface_conditions.join("；")}</strong></div>
      {/if}
      {#if detail.crafting_speed != null}
        <div class="hc-row"><span>速度</span><strong>{detail.crafting_speed}</strong></div>
      {/if}
      {#if detail.module_slots != null}
        <div class="hc-row"><span>模块槽</span><strong>{detail.module_slots}</strong></div>
      {/if}
      {#if detail.energy_usage_j != null}
        <div class="hc-row"><span>能耗</span><strong>{formatPower(detail.energy_usage_j)}</strong></div>
      {/if}
      {#if detail.default_temperature != null}
        <div class="hc-row"><span>默认温度</span><strong>{detail.default_temperature}°C</strong></div>
      {/if}
      {#if detail.quality_level != null}
        <div class="hc-row"><span>等级</span><strong>Lv.{detail.quality_level}</strong></div>
        {#if detail.quality_next}
          <div class="hc-row">
            <span>下一品质</span>
            <strong>{detail.quality_next}
              {#if detail.quality_next_probability != null}
                （{Math.round(detail.quality_next_probability * 100)}%）
              {/if}
            </strong>
          </div>
        {/if}
        {#if detail.quality_crafting_speed != null}
          <div class="hc-row"><span>机器速度</span><strong>{formatMultiplier(detail.quality_crafting_speed)}</strong></div>
        {/if}
        {#if detail.quality_module_speed != null}
          <div class="hc-row"><span>模块速度</span><strong>{formatMultiplier(detail.quality_module_speed)}</strong></div>
        {/if}
        {#if detail.quality_module_productivity != null}
          <div class="hc-row"><span>模块产出</span><strong>{formatMultiplier(detail.quality_module_productivity)}</strong></div>
        {/if}
      {/if}
      {#if detail.ingredients.length > 0}
        <div class="hc-flow">
          <span class="hc-label">原料</span>
          <div class="hc-flows">
            {#each detail.ingredients as flow (flow.name)}
              <span class="hc-flow-item" title={flow.name}>
                <Icon type={flow.kind} name={flow.name} size={20} />
                <span class="hc-flow-copy">
                  <span class="hc-flow-name">{flowName(flow)}</span>
                  <span class="hc-flow-meta">{flowMeta(flow).join(" · ")}</span>
                </span>
              </span>
            {/each}
          </div>
        </div>
      {/if}
      {#if detail.results.length > 0}
        <div class="hc-flow">
          <span class="hc-label">产物</span>
          <div class="hc-flows">
            {#each detail.results as flow (flow.name)}
              <span class="hc-flow-item" title={flow.name}>
                <Icon type={flow.kind} name={flow.name} size={20} />
                <span class="hc-flow-copy">
                  <span class="hc-flow-name">{flowName(flow)}</span>
                  <span class="hc-flow-meta">{flowMeta(flow).join(" · ")}</span>
                </span>
              </span>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .hover-card {
    position: fixed;
    z-index: 60;
    width: min(300px, calc(100vw - 28px));
    max-height: calc(100vh - 24px);
    overflow-y: auto;
    padding: 10px;
    background: var(--panel);
    border: 1px solid var(--accent-line);
    border-radius: var(--radius);
    box-shadow: 0 14px 34px rgba(0, 0, 0, 0.4);
    pointer-events: none;
  }

  .hc-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--line);
  }

  .hc-title {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .hc-title strong {
    overflow: hidden;
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hc-title small {
    color: var(--muted);
    font-size: 10px;
  }

  .hc-body {
    display: grid;
    gap: 3px;
    padding-top: 7px;
  }

  .hc-row {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    color: var(--muted);
    font-size: 11px;
  }

  .hc-row strong {
    color: var(--text);
    font-family: var(--mono);
    font-size: 10px;
    text-align: right;
  }

  .hc-flow {
    display: grid;
    gap: 4px;
    margin-top: 3px;
  }

  .hc-label {
    color: var(--muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .hc-flows {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .hc-flow-item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 6px 2px 2px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .hc-flow-copy {
    display: grid;
    gap: 1px;
    min-width: 0;
  }

  .hc-flow-name {
    overflow: hidden;
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hc-flow-meta {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 9px;
  }
</style>
