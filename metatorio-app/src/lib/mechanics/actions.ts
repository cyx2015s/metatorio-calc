import { allowedModules } from '../runtime/client';
import { itemOf } from '../runtime/types';
import type { CatalogKind, DualVar, MechanicId } from '../runtime/types';
import type { MechanicScope } from '../runtime/mechanic-commands';
import type { runtime as Runtime } from '../runtime/store.svelte';

export type MechanicPickKind = CatalogKind | 'beacon-module';
export interface MechanicSelection {
  kind: CatalogKind;
  title: string;
  kinds: {kind: CatalogKind; label: string}[];
  categoryFilter?: string[];
  tilePurpose?: 'extract' | 'dispose';
  allowedNames?: string[];
  initialName?: string;
  initialQuality?: string;
  onSelect: (name: string, quality: string) => void;
}
export interface MechanicFlowSelection {
  title: string;
  initialTab?: string;
  initialName?: string;
  initialQuality?: string;
  allowedNames?: string[];
  onSelectFlow: (flow: DualVar) => void;
}
export interface MechanicEditorHost {
  select(request: MechanicSelection): void;
  selectFlow(request: MechanicFlowSelection): void;
  notice(message: string): void;
}

/** Reusable editor intents. Views emit IDs, without implementing selector policies. */
export function createMechanicActions(runtime: typeof Runtime, scope: MechanicScope, ui: MechanicEditorHost) {

  const commands = runtime.mechanicCommands(scope);
  scope = commands.scope;
  const isCurrent = () => runtime.selectedProjectId === scope.project && (scope.kind === "universe" ? runtime.selectedUniverseId : runtime.selectedFactoryId) === scope.factory;
  const getEntry = (id: MechanicId) => runtime.mechanicFactory(scope)?.mechanics.find(entry => entry.id === id);
  const showNotice = ui.notice;
  function openSelector(kind: CatalogKind, title: string, onSelect: (name: string, quality: string) => void,
    kinds: {kind: CatalogKind; label: string}[] = [], categoryFilter?: string[], allowedNames?: string[],
    initialName?: string, initialQuality?: string, tilePurpose?: 'extract' | 'dispose') {
    if (!isCurrent()) return;
    ui.select({kind, title, onSelect, kinds, categoryFilter, allowedNames, initialName, initialQuality, tilePurpose});
  }
  function openFlowSelector(request: MechanicFlowSelection) {
    if (isCurrent()) ui.selectFlow(request);
  }
  async function beaconModuleSlots(beaconId: string | undefined): Promise<number> {
    if (!beaconId) return 2;
    const detail = await runtime.getDetail('beacon', beaconId);
    return detail?.beacon_module_slots ?? detail?.module_slots ?? 2;
  }
  function machineModuleSlots(machineId: string | undefined): number {
    return runtime.catalogIndex?.entries.find(entry => entry.kind === 'machine' && entry.name === machineId)?.module_slots ?? 0;
  }
  // ── 机制拾取器分发 ──────────────────────────────────────────────
  async function pick(
    mechanic: MechanicId,
    kind: CatalogKind | "beacon-module",
    a?: number,
    b?: number,
  ) {
    const entry = getEntry(mechanic);
    try {
      switch (kind) {
        case "recipe":
          openSelector(
            "recipe",
            "选择配方",
            (name, quality) => commands.setRecipe(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.recipe?.id,
            entry?.mechanic.recipe?.quality,
          );
          break;
        case "machine": {
          // 机器列表按当前配方的 categories 过滤（组装机/电炉/火箭发射井等）
          const recipeId = entry?.mechanic.recipe?.id;
          const filter = recipeId
            ? (await runtime.getDetail("recipe", recipeId))?.categories ?? []
            : [];
          openSelector(
            "machine",
            "选择机器",
            (name, quality) => commands.setMachine(mechanic, name, quality),
            [{ kind: "machine", label: "制造机" }],
            filter,
            undefined,
            entry?.mechanic.machine?.id,
            entry?.mechanic.machine?.quality,
          );
          break;
        }
        case "offshore-pump": {
          openSelector(
            "offshore-pump",
            "选择抽水机",
            (name, quality) => commands.setMachine(mechanic, name, quality),
            [{ kind: "offshore-pump", label: "抽水机" }],
            undefined,
            undefined,
            entry?.mechanic.machine?.id,
            entry?.mechanic.machine?.quality,
          );
          break;
        }
        case "tile": {
          // 地格：抽取与销毁的判据不同，标题与动作跟着变。
          const isDispose = entry?.mechanic.type === "tile-dispose";
          openSelector(
            "tile",
            isDispose ? "选择可销毁物品的地格" : "选择地格（决定产出哪种流体）",
            (name) =>
              isDispose
                ? commands.setTileDisposeTile(mechanic, name)
                : commands.setTileExtractTile(mechanic, name),
            [{ kind: "tile", label: "地格" }],
            undefined,
            undefined,
            entry?.mechanic.tile,
            undefined,
            isDispose ? "dispose" : "extract",
          );
          break;
        }
        case "mining-machine": {
          // 采矿机列表按当前资源的 category 过滤
          const resource = entry?.mechanic.resource;
          const filter = resource
            ? (await runtime.getDetail("resource", resource))?.categories ?? []
            : [];
          openSelector(
            "mining-machine",
            "选择采矿机",
            (name, quality) => commands.setMachine(mechanic, name, quality),
            [{ kind: "mining-machine", label: "采矿机" }],
            filter,
            undefined,
            entry?.mechanic.machine?.id,
            entry?.mechanic.machine?.quality,
          );
          break;
        }
        case "resource": {
          // 资源列表按当前采矿机的 resource_categories 过滤
          const machineId = entry?.mechanic.machine?.id;
          const filter = machineId
            ? (await runtime.getDetail("mining-machine", machineId))?.categories ?? []
            : [];
          openSelector(
            "resource",
            "选择资源",
            (name) => commands.setResource(mechanic, name),
            [{ kind: "resource", label: "资源" }],
            filter,
            undefined,
            entry?.mechanic.resource,
          );
          break;
        }
        case "fluid": {
          // 按机制类型过滤：流体燃料/流体热有专用标签；发电机/锅炉按流体箱过滤
          const mechKind = entry?.mechanic.type;
          if (mechKind === "fluid-fuel") {
            openSelector(
              "fluid",
              "选择热值流体（燃料）",
              (name) => commands.setFluidFuel(mechanic, name),
              [],
              ["fluid-fuel"],
            );
            break;
          }
          if (mechKind === "fluid-heat") {
            openSelector(
              "fluid",
              "选择提热流体",
              (name) => commands.setFluidHeat(mechanic, name),
              [],
              ["fluid-heat"],
            );
            break;
          }
          const entityId =
            mechKind === "generator"
              ? entry?.mechanic.generator?.id
              : mechKind === "boiler"
                ? entry?.mechanic.boiler?.id
                : undefined;
          const filter = entityId ? await fluidFilterOf(mechKind, entityId) : [];
          openSelector(
            "fluid",
            "选择流体",
            (name) => commands.setFluid(mechanic, name),
            [],
            [],
            filter,
          );
          break;
        }
        case "item": {
          // 按机制类型过滤物品：变质/种植/燃料/发射
          const mechKind = entry?.mechanic.type;
          const filter =
            entry?.transfer ? [] : mechKind === "virtual-science" ? ["science-pack"] : mechKind === "spoil"
              ? ["spoilable"]
              : mechKind === "plant"
                ? ["plantable"]
                : mechKind === "item-fuel"
                  ? ["fuel"]
                  : mechKind === "item-launch"
                    ? ["launchable"]
                    : [];
          const title =
            mechKind === "virtual-science" ? "选择科研包" : mechKind === "spoil"
              ? "选择会变质的物品"
              : mechKind === "plant"
                ? "选择种子"
                : mechKind === "item-fuel"
                  ? "选择燃料"
                  : mechKind === "item-launch"
                    ? "选择发射物品"
                    : "选择物品";
          openSelector(
            "item",
            title,
            (name, quality) => commands.setItem(mechanic, name, quality),
            [],
            filter,
            undefined,
            entry?.mechanic.item?.id,
            entry?.mechanic.item?.quality,
          );
          break;
        }
        case "generator":
          openSelector(
            "generator",
            "选择发电机",
            (name, quality) => commands.setGenerator(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.generator?.id,
            entry?.mechanic.generator?.quality,
          );
          break;
        case "boiler":
          openSelector(
            "boiler",
            "选择锅炉",
            (name, quality) => commands.setBoiler(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.boiler?.id,
            entry?.mechanic.boiler?.quality,
          );
          break;
        case "reactor":
          openSelector(
            "reactor",
            "选择反应堆",
            (name, quality) => commands.setReactor(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.reactor?.id,
            entry?.mechanic.reactor?.quality,
          );
          break;
        case "solar-panel":
          openSelector(
            "solar-panel",
            "选择太阳能板",
            (name, quality) => commands.setSolarPanel(mechanic, name, quality),
            [],
            [],
            undefined,
            entry?.mechanic.solar_panel?.id,
            entry?.mechanic.solar_panel?.quality,
          );
          break;
        case "accumulator":
          openSelector(
            "accumulator",
            "选择蓄电器",
            (name, quality) => commands.setAccumulator(mechanic, name, quality),
          );
          break;
        case "module": {
          // 机器插件：按机器允许的插件鉴权（类别 + 效果类型 + 配方开关）
          // 过滤；添加前校验槽数。
          const machineId = entry?.mechanic.machine?.id;
          const detailKind = entry?.mechanic.type === "mining" ? "mining-machine" : "machine";
          const slots = machineModuleSlots(machineId);
          const filled = entry?.mechanic.module_config?.modules.length ?? 0;
          if (a != null && a >= filled && slots > 0 && filled >= slots) {
            showNotice(`插件槽已满（${slots} 个）`);
            break;
          }
          const recipeId = entry?.mechanic.type === "recipe" ? (entry?.mechanic.recipe?.id ?? null) : null;
          const allowed = machineId
            ? await allowedModules(detailKind, machineId, recipeId, runtime.effectiveContextId)
            : [];
          // 已选机器但鉴权后无可用插件：用不可能匹配的哨兵让选择器显示空。
          const allowedNames =
            machineId && allowed.length === 0 ? [""] : allowed.length > 0 ? allowed : undefined;
          // 编辑已有槽位：预选当前插件（只改品质也可确认）。
          const currentModule =
            a != null ? entry?.mechanic.module_config?.modules[a] : undefined;
          openSelector(
            "module",
            machineId ? "选择插件" : "选择插件（先选择机器）",
            (name, quality) => commands.setModuleSlot(mechanic, a ?? 0, name, quality),
            [],
            [],
            allowedNames,
            currentModule?.id,
            currentModule?.quality,
          );
          break;
        }
        case "beacon": {
          const beacon = a ?? 0;
          openSelector("beacon", "选择插件塔", (name, quality) =>
            commands
              .moduleMessage(mechanic, {
                "set-beacon": { beacon, value: { id: name, quality } },
              })
              .catch(() => {}),
          );
          break;
        }
        case "beacon-module": {
          const beacon = a ?? 0;
          const moduleIdx = b ?? 0;
          const beaconCfg = entry?.mechanic.module_config?.beacons[beacon];
          const beaconModuleCount = beaconCfg?.modules.length ?? 0;
          // 添加新塔内插件前校验：总数量 ≤ 插件塔插件槽数 × 插件塔数量。
          if (moduleIdx >= beaconModuleCount && beaconCfg) {
            const beaconSlots =
              (await beaconModuleSlots(beaconCfg.beacon.id)) * beaconCfg.count;
            const total = beaconCfg.modules.reduce((sum, [, count]) => sum + count, 0);
            if (beaconSlots > 0 && total >= beaconSlots) {
              showNotice(`插件塔插件已满（${beaconSlots} 个槽位）`);
              break;
            }
          }
          // 塔内插件按该插件塔允许的插件鉴权过滤（类别 + 效果类型；与机器同理）
          const beaconId = beaconCfg?.beacon.id;
          const allowed = beaconId
            ? await allowedModules("beacon", beaconId, null, runtime.effectiveContextId)
            : [];
          // 已选插件塔但鉴权后无可用插件：用不可能匹配的哨兵让选择器显示空。
          const allowedNames =
            beaconId && allowed.length === 0 ? [""] : allowed.length > 0 ? allowed : undefined;
          openSelector(
            "module",
            beaconId ? "选择塔内插件" : "选择塔内插件（先选择插件塔）",
            (name, quality) => {
              const value = { id: name, quality };
              if (moduleIdx >= beaconModuleCount) {
                commands
                  .moduleMessage(mechanic, { "add-beacon-module": { beacon, module: value } })
                  .catch(() => {});
              } else {
                commands
                  .moduleMessage(mechanic, {
                    "set-beacon-module": { beacon, module: moduleIdx, value },
                  })
                  .catch(() => {});
              }
            },
            [],
            [],
            allowedNames,
          );
          break;
        }
        default:
          break;
      }
    } catch {
      /* 过滤信息拉取失败时仍打开选择器（不过滤） */
    }
  }

  /** 发电机/锅炉允许的流体（流体箱 filter；无则空 = 不限制）。 */
  async function fluidFilterOf(mechKind: string | undefined, entityId: string): Promise<string[]> {
    const detailKind = mechKind === "generator" ? "generator" : "boiler";
    const detail = await runtime.getDetail(detailKind, entityId);
    return detail?.fluid_filter ? [detail.fluid_filter] : [];
  }

  /** 指定机制燃料：流选择器（物品/流体页签）→ SetFuel。
   *  burner 机器只吃物品燃料：机器的燃料类别（burner_fuel_categories）与
   *  物品的燃料类别（fuel_categories，可多个）**有交集**才兼容（机器无类别
   *  限制 → 全部燃料）；非 burner（流体/热能量源）不做物品限定，流体页签
   *  可选。锅炉/反应堆/采矿机与制造机同规则。 */
  async function pickFuel(mechanic: MechanicId) {
    const entry = getEntry(mechanic);
    const m = entry?.mechanic;
    const type = m?.type;
    // 依机制类型确定要查的实体详情 kind + 设备 id（各组件字段不同）。
    let entityKind: CatalogKind | null = null;
    let entityId: string | null = null;
    if (type === "recipe") {
      entityKind = "machine";
      entityId = m?.machine?.id ?? null;
    } else if (type === "mining") {
      entityKind = "mining-machine";
      entityId = m?.machine?.id ?? null;
    } else if (type === "boiler") {
      entityKind = "boiler";
      entityId = m?.boiler?.id ?? null;
    } else if (type === "reactor") {
      entityKind = "reactor";
      entityId = m?.reactor?.id ?? null;
    }

    let allowedNames: string[] | undefined;
    let initialTab: string | undefined;
    if (entityKind && entityId) {
      const detail = await runtime.getDetail(entityKind, entityId);
      const categories = detail?.burner_fuel_categories ?? [];
      const energySource = detail?.machine_energy_source;
      if (energySource === "burner" || categories.length > 0) {
        // burner 机器：只允许物品燃料（有热值），且（无类别限制 或 类别匹配）。
        allowedNames = (runtime.catalogIndex?.entries ?? [])
          .filter(
            (candidate) =>
              candidate.kind === "item" &&
              candidate.fuel_value_j != null &&
              candidate.fuel_value_j > 0 &&
              (categories.length === 0 ||
                candidate.fuel_categories.some((category) => categories.includes(category))),
          )
          .map((candidate) => candidate.name);
        initialTab = "item";
      } else if (energySource === "fluid") {
        // 流体能量源（如某些锅炉/发电机）：只允许有热值的流体燃料。
        allowedNames = (runtime.catalogIndex?.entries ?? [])
          .filter(
            (candidate) =>
              candidate.kind === "fluid" &&
              candidate.fuel_value_j != null &&
              candidate.fuel_value_j > 0,
          )
          .map((candidate) => candidate.name);
        initialTab = "fluid";
      }
    }
    openFlowSelector({
      title: "选择燃料",
      initialTab,
      allowedNames,
      onSelectFlow: (flow) => {
        const item = itemOf(flow);
        if (item) {
          // 物品燃料：带品质。
          commands.setFuel(mechanic, { kind: "item", item }).catch(() => {});
          return;
        }
        if (flow !== null && typeof flow === "object" && "Fluid" in flow) {
          const fluid = (flow as { Fluid: { name: string } }).Fluid.name;
          commands.setFuel(mechanic, { kind: "fluid", fluid }).catch(() => {});
          return;
        }
        commands.setFuel(mechanic, null).catch(() => {});
      },
    });
  }

  /** 添加插件塔：直接打开插件塔选择器（不再先加空配置行），重复插件塔拒绝。 */
  function addBeacon(mechanic: MechanicId) {
    openSelector("beacon", "选择插件塔（添加）", (name, quality) => {
      const existing = getEntry(mechanic)?.mechanic.module_config?.beacons ?? [];
      // 重复按 IdWithQuality 判等：同种插件塔不同品质允许并存
      if (
        existing.some(
          (beacon) => beacon.beacon.id === name && beacon.beacon.quality === quality,
        )
      ) {
        showNotice(`插件塔 ${name}（${runtime.localizedName("quality", quality)}）已添加，不能重复`);
        return;
      }
      commands
        .moduleMessage(mechanic, { "add-beacon": { beacon: { id: name, quality } } })
        .catch(() => {});
    });
  }

  async function loadFlow(id: number, mechanic: import('../runtime/types').Mechanic) {
    if (scope.kind !== 'universe') return runtime.loadMechanicFlow(scope.project, scope.factory, id, mechanic);
    const preview = await runtime.loadUniversePreview(scope.project, scope.factory);
    const entry = preview.mechanics.find(e => e.id === id);
    if (entry?.error) throw new Error(entry.error);
    return (entry?.flows ?? []).map(f => ({flow: f.flow.flow, location: f.flow.location, amount: f.amount}));
  }
  const setLocation = (id: number, location: import('../runtime/types').Location) => runtime.dispatchUniverse(scope.project, scope.factory, {'set-mechanic-location': {mechanic: id, location}});
  return {commands, pick, pickFuel, addBeacon, loadFlow, setLocation};
}
export type MechanicActions = ReturnType<typeof createMechanicActions>;
