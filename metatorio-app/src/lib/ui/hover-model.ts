import type { CalculationStat, PrototypeDetail } from "$lib/runtime/types";
import { compactNumber } from "$lib/format";

export const statLabels: Record<string, string> = {
  "craft-time": "制作时间", "mining-time": "开采时间", "productivity-cap": "产能加成上限",
  "crafting-speed": "制作速度", "mining-speed": "采矿速度", "module-slots": "插件槽", "power": "额定功耗",
  "pumping-rate": "抽取速率", "resource-drain": "资源消耗率", "heat-capacity": "比热容 / 单位",
  "fuel-value": "燃料热值", "effect-speed": "速度加成", "effect-productivity": "产能加成", "effect-quality": "品质概率加成",
  "effect-consumption": "能耗加成", "effect-pollution": "污染加成", "beacon-effectivity": "信标分配效率",
  "beacon-profile-1": "1 塔衰减系数", "beacon-profile-4": "4 塔衰减系数", "beacon-profile-8": "8 塔衰减系数",
  "buffer-capacity": "储能容量", "science-value": "每瓶科研单位", "solar-peak": "峰值发电（未计星球）",
  "crafting-multiplier": "制造机速度倍率", "module-speed-multiplier": "速度插件倍率",
  "module-productivity-multiplier": "产能插件倍率", "science-multiplier": "科研容量倍率", "accumulator-multiplier": "蓄电容量倍率",
  "stack-size": "堆叠数量", "spoil-time": "变质时间", "max-output": "最大发电功率", "heat-output": "额定热功率",
  "efficiency": "效率", "max-temperature": "最高温度", "default-temperature": "默认温度", "target-temperature": "目标温度",
  "fluid-usage": "流体消耗", "neighbour-bonus": "相邻加成", "quality-level": "品质等级",
};
export function formatStat(stat: CalculationStat): string {
  const {value, unit} = stat;
  if (!Number.isFinite(value)) return "—";
  if (unit === "percent") return `${value > 0 && stat.key.startsWith("effect-") ? "+" : ""}${compactNumber(value * 100)}%`;
  if (unit === "multiplier") return `×${compactNumber(value)}`;
  if (["watts", "joules", "joules-per-degree"].includes(unit)) {
    const suffix = unit === "watts" ? "W" : unit === "joules" ? "J" : "J/℃";
    const prefixes = [[1e12,"T"],[1e9,"G"],[1e6,"M"],[1e3,"k"]] as const;
    const prefix = prefixes.find(([threshold]) => Math.abs(value) >= threshold);
    return `${compactNumber(prefix ? value / prefix[0] : value)} ${prefix?.[1] ?? ""}${suffix}`;
  }
  const suffix = { seconds: "s", watts: "W", joules: "J", "joules-per-degree": "J/℃", "per-second": "/s", number: "" }[unit];
  return `${compactNumber(value)}${suffix ? ` ${suffix}` : ""}`;
}
/** Existing scalar fields remain readable while the wire format grows additively. */
export function detailStats(detail: PrototypeDetail | null, quality = "normal"): CalculationStat[] {
  if (!detail) return [];
  const fallback: CalculationStat[] = [];
  const add = (key: string, value: number | null | undefined, unit: CalculationStat["unit"], factor = 1) => {
    if (value != null) fallback.push({key, value:value * factor, unit});
  };
  add("stack-size", detail.stack_size, "number"); add("fuel-value", detail.fuel_value_j, "joules");
  add("spoil-time", detail.spoil_ticks, "seconds", 1/60);
  add("craft-time", detail.energy_required, "seconds"); add("productivity-cap", detail.maximum_productivity, "percent");
  add("crafting-speed", detail.crafting_speed, "number"); add("module-slots", detail.module_slots ?? detail.beacon_module_slots, "number");
  add("power", detail.energy_usage_j ?? detail.energy_consumption_j, "watts", 60);
  add("max-output", detail.max_power_output_j, "watts", 60); add("heat-output", detail.heat_output_j, "watts", 60);
  add("efficiency", detail.effectivity, "multiplier"); add("max-temperature", detail.maximum_temperature, "number");
  add("default-temperature", detail.default_temperature, "number"); add("target-temperature", detail.target_temperature, "number");
  add("fluid-usage", detail.fluid_usage_per_tick, "per-second", 60); add("neighbour-bonus", detail.neighbour_bonus, "percent");
  add("quality-level", detail.quality_level, "number");
  const calculation = detail.calculation;
  const variant = calculation?.qualities.find((entry) => entry.quality === quality);
  const merged = new Map([...fallback, ...(calculation?.stats ?? []), ...(variant?.stats ?? [])].map((stat) => [stat.key, stat]));
  if (merged.has("mining-time")) merged.delete("craft-time");
  return [...merged.values()];
}
