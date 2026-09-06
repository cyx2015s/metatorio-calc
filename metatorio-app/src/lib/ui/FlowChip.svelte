<script lang="ts">
  // 统一流展示 chip：机制卡/求解结果共用。
  // 图标用正常大小的 HoverIcon（悬停详情 + 品质角标），数量带 +- 号，
  // 能量流用功率文本（W/kW/MW）。
  import HoverIcon from "./HoverIcon.svelte";
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { dualVarLabel } from "$lib/runtime/types";
  import { signedCompactNumber } from "$lib/format";
  import type { DualVar } from "$lib/runtime/types";

  let {
    flow,
    amount,
    scale = 1,
    size = 22,
  }: {
    flow: DualVar;
    /** 每秒流量（正 = 产出，负 = 消耗）。 */
    amount: number;
    /** 求解用量缩放（无求解结果时为 1）。 */
    scale?: number;
    /** 图标边长。 */
    size?: number;
  } = $props();

  /** 流 → 图标 (type, name, detailKind, quality)。 */
  function flowMeta(flow: DualVar): {
    type: string;
    name: string;
    detailKind?: string;
    quality?: string;
  } {
    if (flow !== null && typeof flow === "object") {
      if ("Item" in flow) {
        const item = (flow as { Item: { id: string; quality?: string } }).Item;
        return {
          type: "item",
          name: item.id,
          detailKind: "item",
          quality: item.quality && item.quality !== "normal" ? item.quality : undefined,
        };
      }
      if ("Fluid" in flow) {
        const fluid = (flow as { Fluid: { name: string } }).Fluid;
        return { type: "fluid", name: fluid.name, detailKind: "fluid" };
      }
      if ("Entity" in flow) {
        const entity = (flow as { Entity: { id: string } }).Entity;
        return { type: "entity", name: entity.id, detailKind: "entity" };
      }
      if ("ItemFuel" in flow) return { type: "flow", name: "ItemFuel" };
      if ("FluidFuel" in flow) return { type: "flow", name: "FluidFuel" };
      if ("FluidHeat" in flow) return { type: "flow", name: "FluidHeat" };
      if ("Pollution" in flow) return { type: "flow", name: "Pollution" };
    }
    if (typeof flow === "string") {
      if (flow === "Electricity") return { type: "flow", name: "Electricity" };
      if (flow === "Heat") return { type: "flow", name: "Heat" };
      if (flow === "RocketSlotCapacity") return { type: "flow", name: "RocketSlotCapacity" };
      if (flow === "RocketWeightCapacity") return { type: "flow", name: "RocketWeightCapacity" };
    }
    return { type: "flow", name: dualVarLabel(flow) };
  }

  /** 能量类流（数值单位为瓦特 J/s）。 */
  function isEnergyFlow(flow: DualVar): boolean {
    if (typeof flow === "string") return flow === "Electricity" || flow === "Heat";
    if (flow !== null && typeof flow === "object") {
      return "FluidHeat" in flow || "FluidFuel" in flow || "ItemFuel" in flow;
    }
    return false;
  }

  /** 项目时间刻度：秒/分/时。能量（W/kW/MW）始终按秒（功率），其它流按刻度缩放。 */
  const timeScale = $derived(runtime.selectedProject?.settings.time_scale ?? "seconds");
  const timeFactor = $derived(
    timeScale === "minutes" ? 60 : timeScale === "hours" ? 3600 : 1,
  );
  const timeUnit = $derived(
    timeScale === "minutes" ? "/min" : timeScale === "hours" ? "/h" : "/s",
  );

  /** 瓦特 → 功率文本（数值为 J/s，直接换算）。 */
  function powerValue(watts: number): string {
    if (watts >= 1e6) return `${(watts / 1e6).toFixed(2)} MW`;
    if (watts >= 1e3) return `${(watts / 1e3).toFixed(1)} kW`;
    return `${watts.toFixed(0)} W`;
  }

  /** 数量文本：能量流用功率（带符号，按秒），否则按时间刻度缩放的紧凑数（带 +- 和单位）。 */
  function qtyText(flow: DualVar, amount: number, scale: number): string {
    if (isEnergyFlow(flow)) {
      const scaled = amount * scale;
      const prefix = scaled < 0 ? "-" : "+";
      return `${prefix}${powerValue(Math.abs(scaled))}`;
    }
    const scaled = amount * scale * timeFactor;
    // 取一个带单位的紧凑文本：用保留一位的表格数字。
    const num = Math.abs(scaled) >= 1000 ? signedCompactNumber(scaled) : scaled.toFixed(1);
    return `${scaled < 0 ? "-" : "+"}${num}${timeUnit}`;
  }

  const meta = $derived(flowMeta(flow));
  const displayName = $derived(
    meta.type === "item" || meta.type === "fluid" || meta.type === "entity"
      ? runtime.localizedName(meta.type, meta.name)
      : meta.name,
  );
  /** tooltip 用：能量流显示功率文本，否则显示刻度缩放值。 */
  const tipText = $derived(
    isEnergyFlow(flow)
      ? `${powerValue(Math.abs(amount * scale))}J/s`
      : `${(Math.abs(amount) * scale * timeFactor).toFixed(2)}${timeUnit}`,
  );
</script>

<span
  class="flow-chip"
  class:out={amount > 0}
  title={`${displayName}${meta.quality ? `（${meta.quality}）` : ""} ${amount > 0 ? "产出" : "消耗"} ${tipText}`}
>
  <HoverIcon
    type={meta.type}
    name={meta.name}
    {size}
    detailKind={meta.detailKind}
    quality={meta.quality}
  />
  <span class="mono">{qtyText(flow, amount, scale)}</span>
</span>

<style>
  .flow-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 6px 2px 2px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 11px;
  }

  .flow-chip.out {
    border-color: var(--accent-line);
    background: color-mix(in srgb, var(--card) 88%, var(--accent) 5%);
  }

  .flow-chip .mono {
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
  }
</style>
