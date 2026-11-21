import { untrack } from 'svelte';
import type { Location, MechanicEntry } from '../runtime/types';
import { matchesMechanic, type ExpandedFlow, type FlowFilter } from '../mechanic-filter';
import { normalizeSearch } from '../rich-text';
import { locationKey, locationLabel, transportLabel } from './located';
import { mechanicKindLabels, mechanicPrimary } from './presentation';

export interface MechanicWorkspaceSource {
  project: number | null;
  factory: number | null;
  entries: MechanicEntry[];
  expanded: boolean;
  /** A view may request a full index even when the filter panel is closed. */
  needsAllFlows?: boolean;
  environmentKey: string;
  scopeKey?: string;
  localize?: (kind: string, name: string) => string;
}

function machineKindForEntry(entry: MechanicEntry): string {
  const kind = entry.mechanic.type;
  if (kind === 'mining') return 'mining-machine';
  if (kind === 'tile-extract') return 'entity';
  return 'machine';
}

function entrySearchParts(entry: MechanicEntry, localize: (kind: string, name: string) => string): string[] {
  const m = entry.mechanic;
  const parts: string[] = [m.type, mechanicKindLabels[m.type] ?? ''];
  if (entry.transport) parts.push(entry.transport, transportLabel(entry.transport));
  const primary = mechanicPrimary(m);
  if (primary.name) {
    parts.push(primary.name, localize(primary.kind, primary.name));
  }
  if (primary.quality && primary.quality !== 'normal') {
    parts.push(primary.quality, localize('quality', primary.quality));
  }
  if (m.machine?.id) {
    const mk = machineKindForEntry(entry);
    parts.push(m.machine.id, localize(mk, m.machine.id));
    if (m.machine.quality && m.machine.quality !== 'normal') {
      parts.push(m.machine.quality, localize('quality', m.machine.quality));
    }
  }
  if (m.fluid) parts.push(m.fluid, localize('fluid', m.fluid));
  if (m.tile) parts.push(m.tile, localize('tile', m.tile));
  if (m.accumulator?.id) parts.push(m.accumulator.id, localize('accumulator', m.accumulator.id));
  if (m.fuel) {
    if (m.fuel.kind === 'item') parts.push(m.fuel.item.id, localize('item', m.fuel.item.id));
    else parts.push(m.fuel.fluid, localize('fluid', m.fuel.fluid));
  }
  for (const mod of m.module_config?.modules ?? []) {
    parts.push(mod.id, localize('module', mod.id));
  }
  for (const beacon of m.module_config?.beacons ?? []) {
    parts.push(beacon.beacon.id, localize('beacon', beacon.beacon.id));
    for (const [mod] of beacon.modules) {
      parts.push(mod.id, localize('module', mod.id));
    }
  }
  if (entry.location) {
    parts.push(locationKey(entry.location), locationLabel(entry.location, localize));
    if (entry.location.kind === 'planet') parts.push(entry.location.name);
  }
  return parts;
}

/** Shared query and expansion state; the workspace owns its reactive lifecycle. */
export class MechanicWorkspaceState {
  private filters = $state<{input: FlowFilter | null; output: FlowFilter | null}>({input: null, output: null});
  textQuery = $state('');
  locationFilter = $state<Location | null>(null);
  index = $state<Map<number, ExpandedFlow[]>>(new Map());
  failures = $state<number[]>([]);
  completed = $state(0);
  retry = $state(0);
  private source: () => MechanicWorkspaceSource;
  private loadFlow: (project: number, factory: number, id: number, mechanic: MechanicEntry['mechanic']) => Promise<ExpandedFlow[]>;
  private previousScope = '';

  get input() { return this.filters.input; }
  set input(value: FlowFilter | null) { this.filters.input = value; }
  get output() { return this.filters.output; }
  set output(value: FlowFilter | null) { this.filters.output = value; }

  constructor(source: () => MechanicWorkspaceSource,
    load: (project: number, factory: number, id: number, mechanic: MechanicEntry['mechanic']) => Promise<ExpandedFlow[]>) {
    this.source = source;
    this.loadFlow = load;
  }

  syncScope() {
    const {project, factory, scopeKey} = this.source();
    const scope = scopeKey ?? `${project ?? 0}:${factory ?? 0}`;
    if (scope === this.previousScope) return;
    this.previousScope = scope;
    untrack(() => { this.clearFilters(); this.index = new Map(); this.failures = []; this.completed = 0; });
  }

  /** Called by a host effect; its cleanup prevents stale batches from publishing. */
  loadIndex() {
    const {project, factory, entries, expanded, needsAllFlows, environmentKey} = this.source();
    if ((!expanded && !this.flowFilterActive && !needsAllFlows) || project === null || factory === null) return;
    const snapshot = entries.map(entry => ({id: entry.id, mechanic: JSON.parse(JSON.stringify(entry.mechanic))}));
    void environmentKey;
    void this.retry;
    untrack(() => { this.index = new Map(); this.failures = []; this.completed = 0; });
    let alive = true;
    void (async () => {
      for (let offset = 0; offset < snapshot.length && alive; offset += 6) {
        const batch = await Promise.allSettled(snapshot.slice(offset, offset + 6).map(async entry => ({
          id: entry.id, flows: await untrack(() => this.loadFlow(project, factory, entry.id, entry.mechanic)),
        })));
        if (!alive) return;
        const next = new Map(untrack(() => this.index));
        const errors = [...untrack(() => this.failures)];
        batch.forEach((result, i) => {
          if (result.status === 'fulfilled') next.set(result.value.id, result.value.flows);
          else errors.push(snapshot[offset + i].id);
        });
        this.index = next;
        this.failures = errors;
        this.completed = Math.min(offset + batch.length, snapshot.length);
      }
    })();
    return () => { alive = false; };
  }
  get flowFilterActive() { return !!this.input || !!this.output; }
  get active() { return this.flowFilterActive || this.textQuery.trim().length > 0 || this.locationFilter !== null; }
  get pending() {
    return (this.flowFilterActive || this.source().expanded || !!this.source().needsAllFlows)
      && this.completed < this.source().entries.length;
  }
  get matchingIds() {
    const { entries, localize } = this.source();
    const resolve = localize ?? ((_kind: string, name: string) => name);
    const tokens = this.textQuery
      .trim()
      .split(/\s+/)
      .map(token => normalizeSearch(token))
      .filter(Boolean);
    const targetLocationKey = this.locationFilter ? locationKey(this.locationFilter) : null;
    const checkFlow = this.flowFilterActive;
    return entries.filter(entry => {
      if (targetLocationKey !== null && locationKey(entry.location) !== targetLocationKey) return false;
      if (tokens.length > 0) {
        const haystack = normalizeSearch(entrySearchParts(entry, resolve).join(' '));
        if (!tokens.every(token => haystack.includes(token))) return false;
      }
      if (checkFlow) {
        const flows = this.index.get(entry.id);
        if (flows === undefined || !matchesMechanic(flows, this.input, this.output)) return false;
      }
      return true;
    }).map(entry => entry.id);
  }
  get visibleIds() { return this.active ? this.matchingIds : null; }
  setFilter(side: 'input' | 'output', filter: FlowFilter | null) { this[side] = filter; }
  clearFilters() { this.input = null; this.output = null; this.textQuery = ''; this.locationFilter = null; }
}
