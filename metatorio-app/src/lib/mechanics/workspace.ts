import type { MechanicEntry } from '../runtime/types';
import type { MechanicActions } from './actions';
import type { MechanicWorkspaceState } from './workspace.svelte';

/** View preferences are frontend state, separate from factory data and undo history. */
export type MechanicViewMode = 'list';
export const DEFAULT_MECHANIC_VIEW: MechanicViewMode = 'list';
export type MechanicSolutionMap = ReadonlyMap<number, {amount: number; cost: number}>;

/** Shared view contract. Aggregation and layout remain renderer-specific. */
export interface MechanicViewProps {
  project: number;
  factory: number;
  entries: MechanicEntry[];
  visibleIds: number[] | null;
  solutions: MechanicSolutionMap;
  actions: MechanicActions;
  workspace: MechanicWorkspaceState;
}
