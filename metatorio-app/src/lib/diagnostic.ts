import type { RuntimeDiagnostic, DualVar } from './runtime/types';
import { presentFlow } from './dual-var.ts';
import type { Localize } from './display-labels';

function displayableFlow(flow: unknown): flow is DualVar {
  if (typeof flow === 'string') return true;
  if (!flow || typeof flow !== 'object' || Object.keys(flow).length !== 1) return false;
  const [kind, value] = Object.entries(flow)[0];
  if (kind === 'VirtualScience') return typeof value === 'string';
  if (!value || typeof value !== 'object') return false;
  if (kind === 'Item' || kind === 'Entity') return typeof value.id === 'string' && typeof value.quality === 'string';
  if (kind === 'Fluid') return typeof value.name === 'string' && Array.isArray(value.temperature) && value.temperature.length === 2 && value.temperature.every(Number.isFinite);
  if (kind === 'Custom' || kind === 'Pollution') return typeof value.name === 'string';
  if (kind === 'FluidHeat' || kind === 'FluidFuel') return typeof value.filter === 'string';
  if (kind === 'ItemFuel' || kind === 'ItemFuelSupply') return Array.isArray(value.category) && value.category.every((category: unknown) => typeof category === 'string');
  return false;
}

/** Legacy infrastructure errors may still be strings; never parse their text for IDs. */
export function diagnosticFrom(error: unknown): RuntimeDiagnostic {
  if(typeof error==='string'&&error.startsWith('{')){try{const parsed=JSON.parse(error);if(typeof parsed.code==='string'&&typeof parsed.message==='string')error=parsed;}catch{}}
  const record = error && typeof error === 'object' ? error as Partial<RuntimeDiagnostic> : {};
  const located=(value:unknown):import('./runtime/types').LocatedDualVar[]=>Array.isArray(value)?value.filter(v=>v&&typeof v==='object'&&v.location&&(v.location.kind==='space'||v.location.kind==='planet'&&typeof v.location.name==='string')&&displayableFlow(v.flow)):[];
  const flows = (value: unknown): DualVar[] => Array.isArray(value) ? value.filter(displayableFlow) : [];
  return {
    code: typeof record.code === 'string' ? record.code : 'operation-failed',
    message: typeof record.message === 'string' ? record.message : String(error),
    ...(located(record.located_no_provider).length?{located_no_provider:located(record.located_no_provider)}:{}),
    ...(located(record.located_no_consumer).length?{located_no_consumer:located(record.located_no_consumer)}:{}),
    no_provider: flows(record.no_provider), no_consumer: flows(record.no_consumer), flows: flows(record.flows),
  };
}

/** Copy/export retains both localized meaning and a lossless, searchable identity. */
export function diagnosticText(diagnostic: RuntimeDiagnostic, localize: Localize): string {
  const lines = [diagnostic.message];
  for (const [label, flows] of [['无供给', diagnostic.no_provider], ['无消耗', diagnostic.no_consumer], ['关联对象', diagnostic.flows]] as const) {
    for (const flow of flows) lines.push(`${label}：${presentFlow(flow, localize).description} · ${JSON.stringify(flow)}`);
  }
  for(const [label,flows] of [['无供给',diagnostic.located_no_provider??[]],['无消耗',diagnostic.located_no_consumer??[]]] as const)for(const flow of flows)lines.push(`${label}：${JSON.stringify(flow.location)} · ${presentFlow(flow.flow,localize).description} · ${JSON.stringify(flow)}`);
  return lines.join('\n');
}
