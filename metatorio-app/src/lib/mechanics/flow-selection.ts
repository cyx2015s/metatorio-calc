import type { DualVar } from '../runtime/types';

/** Seed a new selector session from the flow being edited. */
export function flowSelectionInitial(flow?:DualVar): {initialTab:string;initialName?:string;initialQuality?:string} {
  const kind=typeof flow==='string'?flow:flow?Object.keys(flow)[0]:'Item';
  const tabs:Record<string,string>={Item:'item',Entity:'entity',Fluid:'fluid',VirtualScience:'virtual-science',Electricity:'electricity',Heat:'heat',RocketSlotCapacity:'rocket-slot',RocketWeightCapacity:'rocket-weight',ItemFuel:'item-fuel',ItemFuelSupply:'item-fuel',FluidHeat:'fluid-heat',FluidFuel:'fluid-fuel',Pollution:'custom',Custom:'custom'};
  const result:{initialTab:string;initialName?:string;initialQuality?:string}={initialTab:tabs[kind]??'item'};
  if(flow && typeof flow==='object') {
    if('Item' in flow || 'Entity' in flow){
      const item=('Item' in flow?flow.Item:flow.Entity) as {id:string;quality:string};
      result.initialName=item.id;result.initialQuality=item.quality;
    }
    else if('Fluid' in flow)result.initialName=(flow.Fluid as {name:string}).name;
    else if('VirtualScience' in flow)result.initialName=String(flow.VirtualScience);
  }
  return result;
}
