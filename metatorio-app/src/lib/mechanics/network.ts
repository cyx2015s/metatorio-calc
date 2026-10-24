import {locationKey, locationLabel, transportLabel} from "./located.ts";
import type { Location, LocatedDualVar } from "../runtime/types";
import type { DualVar, ExternalInput, MechanicEntry, TimeScale } from '../runtime/types';
import type { ExpandedFlow } from '../mechanic-filter';
import type { MechanicSolutionMap } from './workspace';
import { mechanicPrototype, type Localize } from '../display-labels.ts';
import { presentFlow, isEnergyFlow } from '../dual-var.ts';
import { compactNumber } from '../format.ts';
import { mechanicKindLabels } from './presentation.ts';

export interface NetworkNode {
  location?: Location;
  id: string;
  kind: 'mechanic' | 'flow' | 'external';
  label: string;
  search: string;
  utility: boolean;
  entries: MechanicEntry[];
  flow?: DualVar;
  external?: ExternalInput;
  implicit?: boolean;
  target?: boolean;
  disabled?: boolean;
  unused?: boolean;
}
export interface NetworkContribution { mechanic: number; unitAmount: number; amount: number }
export interface NetworkEdge {
  id: string; source: string; target: string; label: string;
  kind: 'flow' | 'collapsed' | 'compatibility' | 'external';
  flow?: DualVar;
  contributions: NetworkContribution[];
  utility: boolean;
}
export interface NetworkModel { nodes: NetworkNode[]; edges: NetworkEdge[]; solved: boolean }

/** Qualities of the pictured prototype, not of an unrelated machine or input. */
export function networkIconQualities(node: NetworkNode): string[] {
  if (node.flow) {
    const quality = presentFlow(node.flow, (_kind, name) => name).quality;
    return quality ? [quality] : [];
  }
  return [...new Set(node.entries.flatMap(({mechanic: m}) => {
    const prototype = m.type === 'recipe' ? m.recipe : m.type === 'solar' ? m.solar_panel
      : m.type === 'generator' ? m.generator : m.type === 'boiler' ? m.boiler
      : m.type === 'reactor' ? m.reactor : m.type === 'plant' ? m.seed
      : ['spoil', 'item-fuel', 'item-launch', 'virtual-science'].includes(m.type) ? m.item : undefined;
    return prototype ? [prototype.quality] : [];
  }))];
}
export interface NetworkSource {
  entries: MechanicEntry[];
  index: ReadonlyMap<number, ExpandedFlow[]>;
  visibleIds: number[] | null;
  solutions: MechanicSolutionMap;
  solved: boolean;
  targets: DualVar[];
  locatedTargets?: LocatedDualVar[];
  locatedImplicitInputs?: LocatedDualVar[];
  locatedExternalInputs?: {id:number;flow:LocatedDualVar;penalty:number}[];
  externalInputs: ExternalInput[];
  implicitInputs: DualVar[];
  localize: Localize;
  timeScale: TimeScale;
  mergeRecipes: boolean;
  collapse: boolean;
  showUtilities: boolean;
  onlyEnabled: boolean;
}

/** Exact identity. Category sets are unordered; quality, temperature and burnt slots matter. */
export function networkFlowKey(flow: DualVar): string {
  const canonical = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(canonical);
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value)
      .sort(([a], [b]) => a.localeCompare(b)).map(([key, child]) => [key,
        key === 'category' && Array.isArray(child) ? [...child].sort() : canonical(child)]));
    return value;
  };
  return JSON.stringify(canonical(flow));
}

export function networkRate(flow: DualVar, amount: number, timeScale: TimeScale): string {
  const value = Math.abs(amount);
  if (isEnergyFlow(flow)) {
    if (value >= 1e9) return `${compactNumber(value / 1e9)} GW`;
    if (value >= 1e6) return `${compactNumber(value / 1e6)} MW`;
    if (value >= 1e3) return `${compactNumber(value / 1e3)} kW`;
    return `${compactNumber(value)} W`;
  }
  const factor = timeScale === 'hours' ? 3600 : timeScale === 'minutes' ? 60 : 1;
  return `${compactNumber(value * factor)}${timeScale === 'hours' ? '/h' : timeScale === 'minutes' ? '/min' : '/s'}`;
}

/** Display only these zero-cost compatibility relations from runtime/add_conversion_flows.
 * These are possible conversions, not solved transfer quantities. Cooling/heat extraction
 * requires prototype data and is deliberately not reconstructed here. */
export function networkCompatible(from: DualVar, to: DualVar): boolean {
  if (typeof from === 'string' || typeof to === 'string') return false;
  if ('Fluid' in from && 'Fluid' in to) {
    const source = from.Fluid as {name: string; temperature: [number, number]};
    const target = to.Fluid as typeof source;
    const a = source.temperature, b = target.temperature;
    return source.name === target.name && a[0] >= b[0] && a[1] <= b[1]
      && (a[0] !== b[0] || a[1] !== b[1]);
  }
  if ('ItemFuelSupply' in from && 'ItemFuel' in to) {
    const a = from.ItemFuelSupply as {category: string[]; has_burnt_result?: boolean};
    const b = to.ItemFuel as typeof a;
    return a.category.some(category => b.category.includes(category)) && (!a.has_burnt_result || !!b.has_burnt_result);
  }
  for (const kind of ['FluidHeat', 'FluidFuel'] as const) {
    if (kind in from && kind in to) {
      const a = (from as Record<string, {filter: string}>)[kind].filter;
      const b = (to as Record<string, {filter: string}>)[kind].filter;
      return !!a && !b;
    }
  }
  return false;
}

function utility(flow: DualVar): boolean {
  return isEnergyFlow(flow) || (typeof flow !== 'string' && 'Pollution' in flow);
}

export function buildNetwork(source: NetworkSource): NetworkModel {
  const nodes = new Map<string, NetworkNode>(), edges = new Map<string, NetworkEdge>();
  const visible = source.visibleIds === null ? null : new Set(source.visibleIds);
  const scopedKey = (flow: DualVar, location?: Location) => `${locationKey(location)}|${networkFlowKey(flow)}`;
  const targetKeys = new Set([...source.targets.map(flow => scopedKey(flow)), ...(source.locatedTargets ?? []).map(f => scopedKey(f.flow, f.location))]);
  // Power/heat goals are the main chain for that factory, not optional clutter.
  const isUtility = (flow: DualVar, location?: Location) => utility(flow) && !targetKeys.has(scopedKey(flow, location));
  const flowNode = (flow: DualVar, location?: Location) => {
    const id = `f:${location ? locationKey(location) + "|" : ""}${networkFlowKey(flow)}`;
    if (!nodes.has(id)) {
      const info = presentFlow(flow, source.localize);
      nodes.set(id, {id, kind: 'flow', label: [info.label, ...info.qualifiers].join('\n'),
        search: `${location ? locationLabel(location,source.localize) : ""} ${info.description} ${networkFlowKey(flow)}`, utility: isUtility(flow, location), entries: [], flow, location,
        target: targetKeys.has(scopedKey(flow, location))});
    }
    return nodes.get(id)!;
  };
  const edge = (from: string, to: string, flow: DualVar, kind: NetworkEdge['kind'] = 'flow') => {
    const id = JSON.stringify([kind, from, to, networkFlowKey(flow)]);
    if (!edges.has(id)) edges.set(id, {id, source: from, target: to, flow, kind,
      contributions: [], label: '', utility: isUtility(flow)});
    return edges.get(id)!;
  };
  for (const entry of source.entries) {
    if (visible && !visible.has(entry.id) || source.onlyEnabled && !entry.enabled) continue;
    const m = entry.mechanic;
    const group = source.mergeRecipes && m.type === 'recipe'
      ? JSON.stringify(['recipe', locationKey(entry.location), m.recipe?.id, m.machine?.id]) : String(entry.id);
    const id = `m:${group}`;
    let node = nodes.get(id);
    if (!node) {
      const ref = mechanicPrototype(m), name = source.localize(ref.kind, ref.name) || m.type;
      node = {id, kind: 'mechanic', label: name, search: '', utility: false, entries: [], location:entry.location};
      nodes.set(id, node);
    }
    node.entries.push(entry);
    for (const item of source.index.get(entry.id) ?? []) {
      if (!Number.isFinite(item.amount) || item.amount === 0 || !source.showUtilities && isUtility(item.flow, item.location ?? entry.location)) continue;
      const flow = flowNode(item.flow, item.location ?? entry.location);
      const link = item.amount < 0 ? edge(flow.id, id, item.flow) : edge(id, flow.id, item.flow);
      const scale = source.solved ? (entry.enabled ? source.solutions.get(entry.id)?.amount ?? 0 : 0) : 1;
      link.contributions.push({mechanic: entry.id, unitAmount: item.amount, amount: item.amount * scale});
    }
  }
  for (const node of nodes.values()) if (node.kind === 'mechanic') {
    node.disabled = node.entries.every(entry => !entry.enabled);
    node.unused = source.solved && node.entries.every(entry => !entry.enabled || !source.solutions.get(entry.id)?.amount);
    node.search = node.entries.map(entry => `${JSON.stringify(entry.mechanic)} ${entry.id}`).join(' ') + ` ${node.label}`;
    if (node.location) node.search += ` ${locationLabel(node.location,source.localize)}`;
    if (node.entries[0].transport) node.search += ` ${transportLabel(node.entries[0].transport)}`;
    node.label += `\n${node.entries[0].transport ? transportLabel(node.entries[0].transport) : mechanicKindLabels[node.entries[0].mechanic.type] ?? node.entries[0].mechanic.type}`;
    if (node.entries.length > 1) node.label += ` · ${node.entries.length} 个机制`;
    const qualities = networkIconQualities(node);
    if (qualities.length > 1 || qualities.some(quality => quality !== 'normal')) {
      node.label += `\n${qualities.map(quality => source.localize('quality', quality)).join(' / ')}`;
    }
    const machine = node.entries[0].mechanic.machine;
    if (machine) node.label += `\n${source.localize('entity', machine.id)}`;
  }
  // Keep isolated goals in the full graph so missing production is visible.
  if (source.visibleIds === null) for (const flow of source.targets) {
    if (source.showUtilities || !isUtility(flow)) flowNode(flow);
  }
  if (source.visibleIds === null) for (const target of source.locatedTargets ?? []) { if (source.showUtilities || !isUtility(target.flow, target.location)) flowNode(target.flow, target.location); }
  const usedFlows = [...nodes.values()].filter(node => node.flow);
  const externals = [
    ...source.externalInputs.map(external => ({flow: external.flow, external, implicit: false, location: undefined as Location | undefined})),
    ...source.implicitInputs.filter(flow => !source.externalInputs.some(input => networkFlowKey(input.flow) === networkFlowKey(flow)))
      .map(flow => ({flow, external: undefined, implicit: true, location: undefined as Location | undefined})),
    ...(source.locatedExternalInputs ?? []).map(input=>({flow:input.flow.flow, location:input.flow.location, external:{...input,flow:input.flow.flow},implicit:false})),
    ...(source.locatedImplicitInputs ?? []).filter(input=>!(source.locatedExternalInputs??[]).some(e=>scopedKey(e.flow.flow,e.flow.location)===scopedKey(input.flow,input.location))).map(input => ({flow: input.flow, location: input.location, external: undefined, implicit: true})),
  ];
  for (const input of externals) {
    if (!source.showUtilities && isUtility(input.flow)) continue;
    if (!usedFlows.some(node => locationKey(node.location) === locationKey(input.location) && (networkFlowKey(node.flow!) === networkFlowKey(input.flow) || networkCompatible(input.flow, node.flow!)))) continue;
    const flow = flowNode(input.flow, input.location), id = input.implicit ? `implicit:${flow.id}` : `external:${input.external!.id}`;
    const info = presentFlow(input.flow, source.localize);
    nodes.set(id, {id, kind: 'external', label: input.implicit ? '隐式输入' : '外部输入',
      search: `${input.location ? locationLabel(input.location,source.localize) : ''} ${info.description} ${input.implicit ? '隐式' : '外部'}`, utility: flow.utility, entries: [], ...input});
    edge(id, flow.id, input.flow, 'external');
  }
  // Do not union overlapping temperature intervals or fuel category sets.
  const flows = [...nodes.values()].filter(node => node.kind === 'flow');
  const families = new Map<string, NetworkNode[]>();
  for (const node of flows) {
    const flow = node.flow!;
    if (typeof flow === 'string') continue;
    const family = 'Fluid' in flow ? `fluid:${(flow.Fluid as {name: string}).name}`
      : 'ItemFuelSupply' in flow || 'ItemFuel' in flow ? 'fuel'
      : 'FluidHeat' in flow ? 'fluid-heat' : 'FluidFuel' in flow ? 'fluid-fuel' : undefined;
    if (family) { const group = families.get(`${locationKey(node.location)}|${family}`) ?? []; group.push(node); families.set(`${locationKey(node.location)}|${family}`, group); }
  }
  for (const group of families.values()) for (const from of group) for (const to of group) {
    if (from.id !== to.id && networkCompatible(from.flow!, to.flow!)) edge(from.id, to.id, from.flow!, 'compatibility').label = '兼容';
  }
  for (const link of edges.values()) if (link.kind === 'flow') {
    link.label = source.solved
      ? networkRate(link.flow!, link.contributions.reduce((sum, c) => sum + Math.abs(c.amount), 0), source.timeScale)
      : `单位 ${[...new Set(link.contributions.map(c => networkRate(link.flow!, c.unitAmount, source.timeScale)))].join(' / ')}`;
  }
  const incomingById = new Map<string, NetworkEdge[]>(), outgoingById = new Map<string, NetworkEdge[]>();
  for (const link of edges.values()) {
    const incoming = incomingById.get(link.target) ?? []; incoming.push(link); incomingById.set(link.target, incoming);
    const outgoing = outgoingById.get(link.source) ?? []; outgoing.push(link); outgoingById.set(link.source, outgoing);
  }
  if (source.collapse) for (const node of flows) {
    if (node.target || node.utility) continue;
    const incoming = incomingById.get(node.id) ?? [];
    const outgoing = outgoingById.get(node.id) ?? [];
    if (incoming.length !== 1 || outgoing.length !== 1) continue;
    const a = incoming[0], b = outgoing[0];
    if (nodes.get(a.source)?.kind !== 'mechanic' || nodes.get(b.target)?.kind !== 'mechanic' || a.source === b.target) continue;
    const combined = edge(a.source, b.target, node.flow!, 'collapsed');
    combined.contributions = [...a.contributions, ...b.contributions];
    combined.label = `${node.label.replaceAll('\n', ' · ')}\n产 ${a.label} → 耗 ${b.label}`;
    edges.delete(a.id); edges.delete(b.id); nodes.delete(node.id);
  }
  return {nodes: [...nodes.values()], edges: [...edges.values()], solved: source.solved};
}

export interface NetworkViewState {
  positions: Map<string, {x: number; y: number}>;
  zoom?: number;
  pan?: {x: number; y: number};
  collapse: boolean;
  showUtilities: boolean;
  onlyEnabled: boolean;
  query: string;
  selected?: string;
  selectedMechanic?: number | null;
  iconScale?: number;
}
export function createNetworkViewState(): NetworkViewState {
  return {positions: new Map(), collapse: true, showUtilities: false, onlyEnabled: false, query: ''};
}
