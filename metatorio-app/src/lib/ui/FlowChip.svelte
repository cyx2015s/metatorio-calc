<script lang="ts">
  import FlowRef from "./FlowRef.svelte";
  import { isEnergyFlow, presentFlow } from "$lib/dual-var";
  // 统一流展示 chip：机制卡/求解结果共用。
  // 图标用正常大小的 HoverIcon（悬停详情 + 品质角标），数量带 +- 号，
  // 能量流用功率文本（W/kW/MW）。
  import { runtime } from "$lib/runtime/store.svelte.ts";
  import { signedCompactNumber } from "$lib/format";
  import type { DualVar } from "$lib/runtime/types";

  let {
    flow,
    amount,
    scale = 1,
    size = 22,
    showName = false,
    solved = false,
  }: {
    flow: DualVar;
    /** 每秒流量（正 = 产出，负 = 消耗）。 */
    amount: number;
    /** 求解用量缩放（无求解结果时为 1）。 */
    scale?: number;
    /** 图标边长。 */
    size?: number;
    showName?: boolean;
    solved?: boolean;
  } = $props();

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
    return `${num}${timeUnit}`;
  }

  const info = $derived(presentFlow(flow, (kind,name)=>runtime.localizedName(kind,name)));
  /** tooltip 用：能量流显示功率文本，否则显示刻度缩放值。 */
  const tipText = $derived(
    isEnergyFlow(flow)
      ? powerValue(Math.abs(amount * scale))
      : `${(Math.abs(amount) * scale * timeFactor).toFixed(2)}${timeUnit}`,
  );
</script>

<span
  class="flow-chip"
  class:out={amount > 0}
  title={`${info.description} ${amount > 0 ? "产出" : "消耗"} ${tipText}`}
>
  <FlowRef {flow} {size} {showName} showQualifiers={showName} flowRate={amount * scale} rateBasis={solved ? "solution" : "unit"} />
  <span class="mono">{qtyText(flow, amount, scale)}</span>
</span>

<style>
  .flow-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 26px;
    padding: 1px 7px 1px 2px;
    background: var(--input-bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 10.5px;
  }


  .flow-chip.out {
    border-color: var(--ok-line);
    background: color-mix(in srgb, var(--input-bg) 94%, var(--ok) 5%);
  }

  .flow-chip .mono {
    color: var(--text);
    font-family: var(--mono);
    font-size: 10.5px;
    font-weight: 550;
  }
</style>
