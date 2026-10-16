export const syntheticIconUrls: Record<string, string> = {
  Electricity: "/icons/electricity.png",
  Heat: "/icons/heat.png",
  FluidHeat: "/icons/fluid-heat.png",
  FluidFuel: "/icons/fluid-fuel.png",
  ItemFuel: "/icons/item-fuel.png",
  RocketSlotCapacity: "/icons/rocket-capacity.png",
  RocketWeightCapacity: "/icons/rocket-capacity.png",
};

/** 原型归类必须与 Rust 图标生成目录一致，机器/资源等实体子类归入 entity。 */
export function iconTypeForKind(kind: string): string {
  if (kind === "module") return "item";
  if (["item", "recipe", "fluid", "quality", "technology", "planet", "surface", "tile", "space-location", "asteroid-chunk", "utility"].includes(kind)) return kind;
  return "entity";
}
