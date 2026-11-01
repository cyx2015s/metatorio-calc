import type { OrbitalTransportCosts } from "./types";

export const DEFAULT_ORBITAL_TRANSPORT_COSTS = {
  rocket_export_per_kg: 64 ** 3,
  orbital_drop_per_stack: 64,
  orbital_drop_spoil_penalty: 64 ** 3,
} as const satisfies OrbitalTransportCosts;
