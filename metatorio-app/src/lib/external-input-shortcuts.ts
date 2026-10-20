import type { AppMessage, DualVar, ExternalInputAction, FactoryDocument } from './runtime/types';

/** Finite, internal per-second/per-watt cost. Never convert with display units. */
export const DISABLED_RESOURCE_PENALTY = 1e12;
export const FREE_ENERGY_FLOWS = ['Electricity', 'Heat'] as const;

interface Scope { project: number; factory: number }
interface Host {
  getFactory(scope: Scope): FactoryDocument | null;
  send(message: AppMessage): Promise<unknown>;
}

/** UI conveniences using existing external-input messages, with a fixed scope. */
export function createExternalInputShortcuts(host: Host, scope: Scope) {
  scope = Object.freeze({...scope});
  const current = () => {
    const factory = host.getFactory(scope);
    if (!factory) throw new Error('目标工厂已不存在');
    return factory;
  };
  const send = (action: ExternalInputAction) => {
    current();
    return host.send({scope: 'factory', action: {...scope, action: {'external-input': action}}});
  };
  async function setCost(flow: DualVar, penalty: number) {
    const existing = current().external_inputs.find(input => JSON.stringify(input.flow) === JSON.stringify(flow));
    if (existing) {
      if (existing.penalty !== penalty) await send({'set-penalty': {input: existing.id, penalty}});
    } else {
      await send({add: {input: {id: 0, flow, penalty}}});
    }
  }
  return {
    setCost,
    async addFreeEnergy() {
      for (const flow of FREE_ENERGY_FLOWS) {
        if (current().external_inputs.some(input => input.flow === flow && input.penalty === 0)) continue;
        await setCost(flow, 0);
      }
    },
  };
}
