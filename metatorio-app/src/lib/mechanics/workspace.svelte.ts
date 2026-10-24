import { untrack } from 'svelte';
import type { MechanicEntry } from '../runtime/types';
import { matchesMechanic, type ExpandedFlow, type FlowFilter } from '../mechanic-filter';

export interface MechanicWorkspaceSource {
  project: number | null;
  factory: number | null;
  entries: MechanicEntry[];
  expanded: boolean;
  /** A view may request a full index even when the filter panel is closed. */
  needsAllFlows?: boolean;
  environmentKey: string;
  scopeKey?: string;
}

/** Shared query and expansion state; the workspace owns its reactive lifecycle. */
export class MechanicWorkspaceState {
  private filters = $state<{input: FlowFilter | null; output: FlowFilter | null}>({input: null, output: null});
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
    if ((!expanded && !this.active && !needsAllFlows) || project === null || factory === null) return;
    const snapshot = entries.map(entry => ({id: entry.id, mechanic: JSON.parse(JSON.stringify(entry.mechanic))}));
    void environmentKey;
    void this.retry;
    untrack(() => { this.index = new Map(); this.failures = []; this.completed = 0; });
    let alive = true;
    void (async () => {
      for (let offset = 0; offset < snapshot.length && alive; offset += 6) {
        const batch = await Promise.allSettled(snapshot.slice(offset, offset + 6).map(async entry => ({
          id: entry.id, flows: await this.loadFlow(project, factory, entry.id, entry.mechanic),
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
  get active() { return !!this.input || !!this.output; }
  get pending() { return this.completed < this.source().entries.length; }
  get matchingIds() {
    return this.source().entries.filter(entry => {
      const flows = this.index.get(entry.id);
      return flows !== undefined && matchesMechanic(flows, this.input, this.output);
    }).map(entry => entry.id);
  }
  get visibleIds() { return this.active ? this.matchingIds : null; }
  setFilter(side: 'input' | 'output', filter: FlowFilter | null) { this[side] = filter; }
  clearFilters() { this.input = null; this.output = null; }
}
