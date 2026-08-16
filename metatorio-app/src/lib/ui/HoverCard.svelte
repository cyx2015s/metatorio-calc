<script lang="ts">
  // 悬停信息卡片：显示原型名称、类型与关键参数（数据来自后端 prototype_detail，
  // 按 (kind, name) 缓存）。
  import Icon from "./Icon.svelte";
  import type { PrototypeDetail } from "$lib/runtime/types";

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
</script>

{#if detail}
  <div
    class="hover-card"
    style={`left:${Math.min(x + 14, window.innerWidth - 280)}px;top:${Math.min(y + 14, window.innerHeight - 220)}px`}
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
                {flow.amount}
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
                {flow.amount}
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
    width: min(260px, calc(100vw - 28px));
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
    flex-wrap: wrap;
    gap: 4px;
  }

  .hc-flow-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 2px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 10px;
  }
</style>
