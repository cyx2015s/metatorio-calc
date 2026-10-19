import type { DualVar, IdWithQuality, Mechanic } from "./runtime/types";
export type Localize = (kind: string, name: string) => string;

const flowLabels: Record<string, string> = {
  Unknown: "未指定", Electricity: "电力", Heat: "热量",
  RocketSlotCapacity: "火箭运力（槽位）", RocketWeightCapacity: "火箭运力（重量）",
  ItemFuel: "物品燃料", ItemFuelSupply: "物品燃料供给", FluidFuel: "流体燃料", FluidHeat: "流体热量",
};

/** Only equivalent prototype namespaces may fall back to one another. */
export function lookupName(names: ReadonlyMap<string, string>, kind: string, name: string): string {
  if (kind === "flow") return flowLabels[name] || name;
  const alias = kind === "planet" ? "space-location" : kind === "space-location" ? "planet"
    : kind === "module" ? "item"
    : ["machine", "mining-machine", "offshore-pump", "generator", "boiler", "reactor", "beacon", "resource", "solar-panel", "accumulator"].includes(kind) ? "entity" : undefined;
  return names.get(`${kind}/${name}`) || (alias && names.get(`${alias}/${name}`)) || name;
}

export function flowName(flow: DualVar, localize: Localize): string {
  if (typeof flow === "string") return flowLabels[flow] || flow;
  const key=Object.keys(flow)[0], value=Object.values(flow)[0];
  if (key === "VirtualScience") return `虚拟科研（${localize("item",String(value))}）`;
  if (key === "Item" || key === "Entity") return localize(key.toLowerCase(),(value as IdWithQuality).id);
  if (key === "Fluid") return localize("fluid",(value as {name:string}).name);
  if (key === "Pollution") return `污染（${localize("airborne-pollutant",(value as {name:string}).name)}）`;
  if (key === "Custom") return (value as {name:string}).name; // User text is not a prototype ID.
  return flowLabels[key] || key;
}

/** Human-facing diagnostics retain quality, temperature and fuel constraints. */
export function describeFlow(flow: DualVar, localize: Localize): string {
  const name=flowName(flow,localize);
  if (typeof flow === "string") return name;
  const key=Object.keys(flow)[0], value=Object.values(flow)[0];
  if (key === "Item" || key === "Entity") {
    const quality=(value as IdWithQuality).quality;
    return name+(quality && quality!=="normal" ? `（${localize("quality",quality)}）` : "");
  }
  if (key === "Fluid") {
    const range=(value as {temperature:[number,number]}).temperature;
    return range ? `${name}（${range[0]===range[1]?range[0]:range.join("～")}℃）` : name;
  }
  if (key === "ItemFuel" || key === "ItemFuelSupply") {
    const fuel=value as {category:string[];has_burnt_result?:boolean};
    const categories=fuel.category.map(id=>localize("fuel-category",id)).join(" / ") || "任意";
    return `${name}（${categories}${fuel.has_burnt_result ? "；有燃烧产物" : ""}）`;
  }
  if (key === "FluidHeat" || key === "FluidFuel") {
    const filter=(value as {filter:string}).filter;
    return `${name}（${filter?localize("fluid",filter):"任意流体"}）`;
  }
  return name;
}

export function mechanicPrototype(mechanic: Mechanic): {kind:string;name:string} {
  switch(mechanic.type) {
    case "recipe": return {kind:"recipe",name:mechanic.recipe?.id || ""};
    case "mining": return {kind:"resource",name:mechanic.resource || ""};
    case "solar": return {kind:"entity",name:mechanic.solar_panel?.id || ""};
    case "tile-extract": return {kind:"tile",name:mechanic.tile || ""};
    case "generator": return {kind:"entity",name:mechanic.generator?.id || ""};
    case "boiler": return {kind:"entity",name:mechanic.boiler?.id || ""};
    case "reactor": return {kind:"entity",name:mechanic.reactor?.id || ""};
    case "fluid-fuel": case "fluid-heat": return {kind:"fluid",name:mechanic.fluid || ""};
    case "plant": return {kind:"item",name:mechanic.seed?.id || ""};
    default: return {kind:"item",name:mechanic.item?.id || ""};
  }
}
