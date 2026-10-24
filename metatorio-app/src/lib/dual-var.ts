import { flowName, type Localize } from './display-labels.ts';
import type { DualVar, IdWithQuality } from './runtime/types';

export interface FlowMatch { quality?: string; temperature?: [number, number] }
export interface FlowPresentation {
  type: string; name: string; detailKind?: string; quality?: string;
  kind: string; kindLabel: string; label: string; identifier: string;
  badge?: string; wildcard: boolean; qualifiers: string[];
  facts: {label: string; value: string}[]; description: string;
}

/** A DualVar is a flow identity, not merely an item icon. Match is optional:
 * absent means an exact flow; {} means a pattern with unconstrained quality/temperature. */
export function presentFlow(flow: DualVar, localize: Localize, match?: FlowMatch): FlowPresentation {
  const kind = typeof flow === 'string' ? flow : Object.keys(flow)[0];
  const value = typeof flow === 'string' ? undefined : Object.values(flow)[0];
  const info: FlowPresentation = {
    type: 'flow', name: kind, kind, kindLabel: '计算流', label: flowName(flow, localize),
    identifier: kind, wildcard: false, qualifiers: [], facts: [], description: '',
  };
  const fact = (label: string, value: string) => { info.facts.push({label, value}); info.qualifiers.push(value); };
  if (kind === 'Item' || kind === 'Entity') {
    const item = value as IdWithQuality;
    info.type = kind.toLowerCase(); info.name = item.id; info.detailKind = info.type;
    info.identifier = item.id; info.kindLabel = kind === 'Item' ? '物品' : '实体';
    info.quality = match ? match.quality : item.quality;
    info.wildcard = !!match && match.quality === undefined;
    fact(match ? '匹配品质' : '品质', info.wildcard ? '任意品质' : localize('quality', info.quality ?? 'normal'));
    if (kind === 'Entity') info.badge = '实体';
  } else if (kind === 'Fluid') {
    const fluid = value as {name: string; temperature: [number, number]};
    info.type = 'fluid'; info.name = fluid.name; info.detailKind = 'fluid'; info.identifier = fluid.name; info.kindLabel = '流体';
    const range = match ? match.temperature : fluid.temperature;
    const text = range ? `${range[0] === range[1] ? range[0] : range.join('～')}℃` : '任意温度';
    info.wildcard = !!match && !range;
    fact(match ? '匹配温度' : '温度', text); info.badge = text;
  } else if (kind === 'VirtualScience') {
    info.type = 'item'; info.name = String(value); info.detailKind = 'item'; info.identifier = String(value);
    info.kindLabel = '虚拟科研'; info.badge = '科研'; fact('计量', '科研价值，不带品质');
  } else if (kind === 'Pollution' || kind === 'Custom') {
    info.identifier = (value as {name: string}).name;
    info.kindLabel = kind === 'Pollution' ? '污染流' : '自定义流';
    fact('流名称', kind === 'Pollution' ? localize('airborne-pollutant', info.identifier) : info.identifier);
  } else if (kind === 'ItemFuel' || kind === 'ItemFuelSupply') {
    const fuel = value as {category: string[]; has_burnt_result?: boolean};
    info.name = 'ItemFuel'; info.kindLabel = kind === 'ItemFuel' ? '抽象燃料需求' : '抽象燃料供给';
    info.badge = kind === 'ItemFuel' ? '需' : '供';
    fact('燃料类别', fuel.category.map(id => localize('fuel-category', id)).join(' / ') || '空类别集合');
    fact(kind === 'ItemFuel' ? '燃尽产物栏' : '燃烧产物', kind === 'ItemFuel'
      ? fuel.has_burnt_result ? '可接收燃尽产物' : '不可接收燃尽产物'
      : fuel.has_burnt_result ? '有燃尽产物' : '无燃尽产物');
  } else if (kind === 'FluidHeat' || kind === 'FluidFuel') {
    const filter = (value as {filter: string}).filter;
    info.kindLabel = kind === 'FluidHeat' ? '流体热量' : '流体燃料';
    fact('流体限制', filter ? localize('fluid', filter) : '任意流体');
  } else if (kind === 'RocketSlotCapacity' || kind === 'RocketWeightCapacity') {
    info.badge = kind === 'RocketSlotCapacity' ? '槽位' : '重量'; info.kindLabel = '火箭运力';
    fact('计量', kind === 'RocketSlotCapacity' ? '堆叠槽位' : '物品重量（g）');
  } else if (kind === 'Unknown') info.kindLabel = '未指定的流';
  info.description = [info.kindLabel, info.label, ...info.qualifiers].join(' · ');
  return info;
}

export function isEnergyFlow(flow: DualVar): boolean {
  const kind = typeof flow === 'string' ? flow : Object.keys(flow)[0];
  return ['Electricity', 'Heat', 'ItemFuel', 'ItemFuelSupply', 'FluidFuel', 'FluidHeat'].includes(kind);
}
