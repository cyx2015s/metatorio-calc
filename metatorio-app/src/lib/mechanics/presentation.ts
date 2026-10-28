import type { CatalogKind, Mechanic } from "../runtime/types";

export const mechanicKindLabels: Record<string, string> = {
    recipe: "配方",
    mining: "采矿",
    spoil: "变质",
    plant: "种植",
    "item-fuel": "物品燃料",
    "virtual-science": "虚拟科研",
    "item-launch": "发射",
    generator: "发电机",
    boiler: "锅炉",
    reactor: "反应堆",
    "fusion-reactor": "聚变反应堆",
    "fusion-generator": "聚变发电机",
    solar: "太阳能",
    "fluid-fuel": "流体燃料",
    "fluid-heat": "流体热",
    "tile-extract": "地格抽取",
    "tile-dispose": "地格销毁",
};

// Catalog selector, primary field, and (when different) localization namespace.
const primaryPrototypes = {
  recipe: ["recipe", "recipe"],
  mining: ["resource", "resource"],
  spoil: ["item", "item"],
  plant: ["item", "seed"],
  "item-fuel": ["item", "item"],
  "virtual-science": ["item", "item"],
  "item-launch": ["item", "item"],
  generator: ["generator", "generator", "entity"],
  "fusion-generator": ["fusion-generator", "generator", "entity"],
  boiler: ["boiler", "boiler", "entity"],
  reactor: ["reactor", "reactor", "entity"],
  "fusion-reactor": ["fusion-reactor", "reactor", "entity"],
  solar: ["solar-panel", "solar_panel", "entity"],
  "fluid-fuel": ["fluid", "fluid"],
  "fluid-heat": ["fluid", "fluid"],
  "tile-extract": ["tile", "tile"],
  "tile-dispose": ["item", "item"],
} as const satisfies Record<Mechanic["type"], readonly [CatalogKind, keyof Mechanic, (CatalogKind | "entity")?]>;

/** One primary identity for editor selection, icons, quality and group headings. */
export function mechanicPrimary(mechanic: Mechanic) {
  const [kind, field, prototypeKind = kind] = primaryPrototypes[mechanic.type];
  const value = mechanic[field];
  return {
    kind, prototypeKind,
    name: typeof value === "string" ? value : value?.id ?? "",
    quality: typeof value === "object" ? value?.quality ?? "normal" : "normal",
  };
}
