import type { DualVar } from './runtime/types';

export interface ExpandedFlow { location?: import("./runtime/types").Location; flow: DualVar; amount: number }
export interface FlowFilter { flow: DualVar; quality?: string; temperature?: [number, number] }

/** Stable object identity; quality and temperature are optional refinements. */
export function flowObjectKey(flow: DualVar): string {
  if (typeof flow === 'string') return flow;
  const [kind, value] = Object.entries(flow)[0];
  if (kind === 'Item' || kind === 'Entity') return JSON.stringify([kind, (value as {id: string}).id]);
  if (kind === 'Fluid' || kind === 'Pollution' || kind === 'Custom') return JSON.stringify([kind, (value as {name: string}).name]);
  if (kind === 'VirtualScience') return JSON.stringify([kind, value]);
  // Abstract fuel constraints remain distinct; array ordering is not semantic.
  if (kind === 'ItemFuel' || kind === 'ItemFuelSupply') {
    const fuel = value as {category: string[]; has_burnt_result?: boolean};
    return JSON.stringify([kind, [...fuel.category].sort(), !!fuel.has_burnt_result]);
  }
  return JSON.stringify(flow);
}

export function matchesFlow(flow: DualVar, filter: FlowFilter): boolean {
  if (flowObjectKey(flow) !== flowObjectKey(filter.flow)) return false;
  if (typeof flow === 'string') return true;
  const value = Object.values(flow)[0] as {quality?: string; temperature?: [number, number]};
  if (filter.quality !== undefined && value.quality !== filter.quality) return false;
  if (filter.temperature !== undefined && (!value.temperature ||
    value.temperature[0] !== filter.temperature[0] || value.temperature[1] !== filter.temperature[1])) return false;
  return true;
}

export function matchesMechanic(flows: ExpandedFlow[], input: FlowFilter | null, output: FlowFilter | null): boolean {
  return (!input || flows.some(entry => entry.amount < 0 && matchesFlow(entry.flow, input))) &&
    (!output || flows.some(entry => entry.amount > 0 && matchesFlow(entry.flow, output)));
}

/** Successful empty expansions are data; failures are retryable, not cached empties. */
export class MechanicFlowCache {
  private entries = new Map<string, {hash: string; promise: Promise<ExpandedFlow[]>}>();
  get(key: string, hash: string, load: () => Promise<ExpandedFlow[]>): Promise<ExpandedFlow[]> {
    const cached = this.entries.get(key);
    if (cached?.hash === hash) return cached.promise;
    const entry = {hash, promise: Promise.resolve().then(load).catch(error => {
      if (this.entries.get(key) === entry) this.entries.delete(key);
      throw error;
    })};
    this.entries.set(key, entry);
    return entry.promise;
  }
  clear(): void { this.entries.clear(); }
}
