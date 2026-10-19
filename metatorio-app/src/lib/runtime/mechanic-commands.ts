import type { AppMessage, FactoryDocument, FactoryId, MechanicId, MechanicKind, ProjectId } from './types';

export interface MechanicScope { readonly project: ProjectId; readonly factory: FactoryId }
export interface MechanicCommandHost {
  send(message: AppMessage): Promise<unknown>;
  getFactory(scope: MechanicScope): FactoryDocument | null;
}

/** All mechanism writes bind their destination once, independently of UI selection. */
export class MechanicCommands {
  readonly scope: MechanicScope;
  private readonly host: MechanicCommandHost;
  constructor(host: MechanicCommandHost, scope: MechanicScope) {
    this.host = host;
    this.scope = Object.freeze({...scope});
  }

  private async send(message: AppMessage): Promise<void> {
    if (!this.host.getFactory(this.scope)) throw new Error('目标工厂已不存在');
    await this.host.send(message);
  }

  async cleanup(action: import('./types').CleanupAction): Promise<void> {
    await this.send({scope: 'factory', action: {...this.scope, action: {cleanup: action}}});
  }

  async cloneMechanic(mechanic: MechanicId): Promise<void> {
    const { project, factory } = this.scope;
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { clone: { mechanic } } } },
    });
  }

  async addMechanic(kind: MechanicKind): Promise<void> {
    const { project, factory } = this.scope;
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { add: { kind } } } },
    });
  }

  async removeMechanic(mechanic: MechanicId): Promise<void> {
    const { project, factory } = this.scope;
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { remove: { mechanic } } } },
    });
  }

  /** 机制排序（position 为目标位置）。 */
  async reorderMechanic(mechanic: MechanicId, position: number): Promise<void> {
    const { project, factory } = this.scope;
    await this.send({
      scope: "factory",
      action: { project, factory, action: { "mechanic-list": { reorder: { mechanic, position } } } },
    });
  }

  /** 拖拽后的整表重排（从末尾往前逐个移动到目标位置，保证稳定）。 */
  async reorderMechanics(order: MechanicId[]): Promise<void> {
    for (let i = order.length - 1; i >= 0; i--) {
      const current = this.host.getFactory(this.scope)?.mechanics ?? [];
      const currentIndex = current.findIndex((entry) => entry.id === order[i]);
      if (currentIndex >= 0 && currentIndex !== i) {
        await this.reorderMechanic(order[i], i);
      }
    }
  }

  async setMechanicEnabled(mechanic: MechanicId, enabled: boolean): Promise<void> {
    const { project, factory } = this.scope;
    await this.send({
      scope: "factory",
      action: {
        project,
        factory,
        action: { "mechanic-list": { "set-enabled": { mechanic, enabled } } },
      },
    });
  }

  async mechanicMessage(mechanic: MechanicId, action: import("./types").MechanicAction): Promise<void> {
    const { project, factory } = this.scope;
    await this.send({
      scope: "factory",
      action: { project, factory, action: { mechanic: { mechanic, action } } },
    });
  }

  /** 当前机制的类型（来自文档快照）；用于构造按类型标签的 action。 */
  mechanicKind(mechanic: MechanicId): import("./types").MechanicKind {
    const kind = this.host.getFactory(this.scope)?.mechanics.find((entry) => entry.id === mechanic)?.mechanic
      .type;
    if (!kind) throw new Error(`机制 #${mechanic} 不存在`);
    return kind;
  }

  async setRecipe(mechanic: MechanicId, recipe: string, quality = "normal"): Promise<void> {
    const recipeId = { id: recipe, quality } as import("./types").IdWithQuality;
    await this.mechanicMessage(mechanic, { recipe: { "set-recipe": { recipe: recipeId } } });
  }

  async setMachine(mechanic: MechanicId, machine: string, quality = "normal"): Promise<void> {
    const machineId = { id: machine, quality } as import("./types").IdWithQuality;
    switch (this.mechanicKind(mechanic)) {
      case "recipe":
        return this.mechanicMessage(mechanic, { recipe: { "set-machine": { machine: machineId } } });
      case "mining":
        return this.mechanicMessage(mechanic, { mining: { "set-machine": { machine: machineId } } });
      case "tile-extract":
        return this.setTileExtractMachine(mechanic, machine, quality);
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持设置机器`);
    }
  }

  async setResource(mechanic: MechanicId, resource: string): Promise<void> {
    await this.mechanicMessage(mechanic, { mining: { "set-resource": { resource } } });
  }

  async setItem(mechanic: MechanicId, item: string, quality = "normal"): Promise<void> {
    const itemId = { id: item, quality } as import("./types").IdWithQuality;
    switch (this.mechanicKind(mechanic)) {
      case "spoil":
        return this.mechanicMessage(mechanic, { spoil: { "set-item": { item: itemId } } });
      case "item-fuel":
        return this.mechanicMessage(mechanic, { "item-fuel": { "set-item": { item: itemId } } });
      case "virtual-science":
        return this.mechanicMessage(mechanic, { "virtual-science": { "set-item": { item: itemId } } });
      case "tile-dispose":
        return this.setTileDisposeItem(mechanic, item, quality);
      case "item-launch":
        return this.mechanicMessage(mechanic, { "item-launch": { "set-item": { item: itemId } } });
      case "plant":
        return this.mechanicMessage(mechanic, { plant: { "set-seed": { seed: itemId } } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持设置物品`);
    }
  }

  async setGenerator(mechanic: MechanicId, generator: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      generator: { "set-generator": { generator: { id: generator, quality } } },
    });
  }

  async setBoiler(mechanic: MechanicId, boiler: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      boiler: { "set-boiler": { boiler: { id: boiler, quality } } },
    });
  }

  /** 锅炉 heat-fluid-inside 模式的输出温度；null = 流体最高温度。 */
  async setBoilerOutputTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    await this.mechanicMessage(mechanic, {
      boiler: { "set-output-temperature": { temperature } },
    });
  }

  /** 流体燃料机制：选择热值流体。 */
  async setFluidFuel(mechanic: MechanicId, fluid: string): Promise<void> {
    await this.mechanicMessage(mechanic, { "fluid-fuel": { "set-fluid": { fluid } } });
  }

  /** 流体热机制：选择提热流体。 */
  async setFluidHeat(mechanic: MechanicId, fluid: string): Promise<void> {
    await this.mechanicMessage(mechanic, { "fluid-heat": { "set-fluid": { fluid } } });
  }

  /** 地格抽取：选择地格（**地格决定产出哪种流体**）。 */
  async setTileExtractTile(mechanic: MechanicId, tile: string): Promise<void> {
    await this.mechanicMessage(mechanic, { "tile-extract": { "set-tile": { tile } } });
  }

  /** 地格抽取：选择抽取机械（决定速率 pumping_speed）。 */
  async setTileExtractMachine(
    mechanic: MechanicId,
    machine: string,
    quality = "normal",
  ): Promise<void> {
    await this.mechanicMessage(mechanic, {
      "tile-extract": { "set-machine": { machine: { id: machine, quality } } },
    });
  }

  /** 地格销毁：选择可销毁物品的地格（后端要求 destroys_dropped_items）。 */
  async setTileDisposeTile(mechanic: MechanicId, tile: string): Promise<void> {
    await this.mechanicMessage(mechanic, { "tile-dispose": { "set-tile": { tile } } });
  }

  /** 地格销毁：选择被销毁的物品。 */
  async setTileDisposeItem(
    mechanic: MechanicId,
    item: string,
    quality = "normal",
  ): Promise<void> {
    await this.mechanicMessage(mechanic, {
      "tile-dispose": { "set-item": { item: { id: item, quality } } },
    });
  }

  /** 设置流体类机制的温度（流体热/发电机/锅炉）。流体燃料只看热值，没有温度可编辑。 */
  async setMechanicTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    if (kind === "fluid-heat") {
      return this.mechanicMessage(mechanic, {
        "fluid-heat": { "set-temperature": { temperature } },
      });
    }
    if (kind === "generator") {
      return this.mechanicMessage(mechanic, { generator: { "set-temperature": { temperature } } });
    }
    if (kind === "boiler") {
      return this.mechanicMessage(mechanic, { boiler: { "set-temperature": { temperature } } });
    }
    throw new Error(`${kind} 机制不支持设置温度`);
  }

  /** 指定机制燃料（配方/采矿/锅炉/反应堆）；null = 自动（燃料类别抽象）。 */
  async setFuel(mechanic: MechanicId, fuel: import("./types").Fuel | null): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    if (kind === "recipe") {
      return this.mechanicMessage(mechanic, { recipe: { "set-fuel": { fuel } } });
    }
    if (kind === "mining") {
      return this.mechanicMessage(mechanic, { mining: { "set-fuel": { fuel } } });
    }
    if (kind === "boiler") {
      return this.mechanicMessage(mechanic, { boiler: { "set-fuel": { fuel } } });
    }
    if (kind === "reactor") {
      return this.mechanicMessage(mechanic, { reactor: { "set-fuel": { fuel } } });
    }
    throw new Error(`${kind} 机制不支持指定燃料`);
  }

  /** 指定机制燃料温度（配方/采矿/锅炉）；null = 默认温度。 */
  async setFuelTemperature(mechanic: MechanicId, temperature: number | null): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    if (kind === "recipe") {
      return this.mechanicMessage(mechanic, { recipe: { "set-fuel-temperature": { temperature } } });
    }
    if (kind === "mining") {
      return this.mechanicMessage(mechanic, { mining: { "set-fuel-temperature": { temperature } } });
    }
    if (kind === "boiler") {
      return this.mechanicMessage(mechanic, { boiler: { "set-fuel-temperature": { temperature } } });
    }
    throw new Error(`${kind} 机制不支持设置燃料温度`);
  }

  /** 反应堆相邻数（0-8）。 */
  async setNeighbours(mechanic: MechanicId, neighbours: number): Promise<void> {
    await this.mechanicMessage(mechanic, { reactor: { "set-neighbours": { neighbours } } });
  }

  /** 火箭发射重量模式（true = 按重量；false = 按堆叠槽位）。 */
  async setWeightMode(mechanic: MechanicId, weight_mode: boolean): Promise<void> {
    await this.mechanicMessage(mechanic, { "item-launch": { "set-weight-mode": { weight_mode } } });
  }

  async setReactor(mechanic: MechanicId, reactor: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      reactor: { "set-reactor": { reactor: { id: reactor, quality } } },
    });
  }

  async setSolarPanel(mechanic: MechanicId, solarPanel: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      solar: { "set-solar-panel": { solar_panel: { id: solarPanel, quality } } },
    });
  }

  async setAccumulator(mechanic: MechanicId, accumulator: string, quality = "normal"): Promise<void> {
    await this.mechanicMessage(mechanic, {
      solar: { "set-accumulator": { accumulator: { id: accumulator, quality } } },
    });
  }

  async setFluid(mechanic: MechanicId, fluid: string): Promise<void> {
    switch (this.mechanicKind(mechanic)) {
      case "generator":
        return this.mechanicMessage(mechanic, { generator: { "set-fluid": { fluid } } });
      case "boiler":
        return this.mechanicMessage(mechanic, { boiler: { "set-fluid": { fluid } } });
      default:
        throw new Error(`${this.mechanicKind(mechanic)} 机制不支持设置流体`);
    }
  }

  /** 模块配置消息（按机制类型包装 recipe/mining）。 */
  async moduleMessage(mechanic: MechanicId, action: import("./types").ModuleAction): Promise<void> {
    const kind = this.mechanicKind(mechanic);
    switch (kind) {
      case "recipe":
        return this.mechanicMessage(mechanic, { recipe: { module: action } });
      case "mining":
        return this.mechanicMessage(mechanic, { mining: { module: action } });
      default:
        throw new Error(`${kind} 机制不支持模块配置`);
    }
  }

  async setModuleSlot(
    mechanic: MechanicId,
    slot: number,
    module: string | null,
    quality = "normal",
  ): Promise<void> {
    const moduleId = module ? { id: module, quality } : null;
    await this.moduleMessage(mechanic, {
      "set-module-slot": { slot, module: moduleId },
    });
  }

  async clearModules(mechanic: MechanicId): Promise<void> {
    await this.moduleMessage(mechanic, "clear-modules");
  }

}
