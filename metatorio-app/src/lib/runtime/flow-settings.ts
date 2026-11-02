import type { ProjectSettings } from './types';

/** Display units, LP backend and transport prices do not alter a local mechanism's coefficients. */
export function mechanicFlowSettings(settings: ProjectSettings | undefined) {
  const { time_scale, solver, orbital_transport_costs, ...physical } = settings ?? {};
  return physical;
}
