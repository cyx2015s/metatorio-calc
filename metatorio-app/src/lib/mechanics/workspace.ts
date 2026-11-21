import type { DualVar, Location, MechanicEntry } from '../runtime/types';
import type { MechanicActions } from './actions';
import type { MechanicWorkspaceState } from './workspace.svelte';
import type { MechanicScrollState } from './view-scroll';

/** View preferences are frontend state, separate from factory data and undo history. */
export type MechanicViewMode = 'list' | 'network';
export const DEFAULT_MECHANIC_VIEW: MechanicViewMode = 'list';
export type MechanicSolutionMap = ReadonlyMap<number, {amount: number; cost: number}>;
export type FlowTraceMode = 'producers' | 'consumers' | 'network' | 'suggest';
export type FlowTraceHandler = (flow: DualVar, mode: FlowTraceMode, location?: Location) => void;
export interface MechanicFocusRequest {
  mechanicId?: number;
  flow?: DualVar;
  location?: Location;
  targetView?: 'list' | 'network';
  seq: number;
}
export interface FlowTraceRequest {
  flow: DualVar;
  side: 'input' | 'output';
  seq: number;
}

/** Shared view contract. Aggregation and layout remain renderer-specific. */
export interface MechanicViewProps {
  project: number;
  factory: number;
  entries: MechanicEntry[];
  visibleIds: number[] | null;
  solutions: MechanicSolutionMap;
  solved: boolean;
  actions: MechanicActions;
  workspace: MechanicWorkspaceState;
  scrollState: MechanicScrollState;
}
