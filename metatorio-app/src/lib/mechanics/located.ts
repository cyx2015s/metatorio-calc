import type { FactoryDocument, Location, MechanicEntry, UniverseDocument } from '../runtime/types';

export const locationKey = (location?: Location) => location ? location.kind === 'space' ? 'space' : `planet:${location.name}` : '';
export const locationLabel = (location: Location, localize: (kind: string, name: string) => string) => location.kind === 'space' ? '太空' : localize('planet', location.name);

/** Presentation projection only. Commands always write LocatedMechanic, never this object. */
export function locatedEntries(universe: UniverseDocument): MechanicEntry[] {
  return universe.mechanics.map(entry => ({id: entry.id, enabled: entry.enabled,
    location: entry.mechanic.location, transport: entry.mechanic.type === 'local' ? undefined : entry.mechanic.type,
    mechanic: entry.mechanic.type === 'local' ? entry.mechanic.mechanic
      : {type: 'item-launch', item: entry.mechanic.item, weight_mode: true},
  }));
}
export function universeView(universe: UniverseDocument): FactoryDocument {
  return {id: universe.id, name: universe.name, mechanics: locatedEntries(universe),
    settings: {planet: null, surface: null, debug: false}, external_inputs: (universe.external_inputs??[]).map(i=>({...i,location:i.flow.location,flow:i.flow.flow})),
    targets: universe.targets.map(t => ({...t, location: t.flow.location, flow: t.flow.flow})),
    target_expressions: universe.target_expressions.map(e => ({...e, terms: e.terms.map(t => ({...t, location: t.flow.location, flow: t.flow.flow}))})),
    strict_source: universe.strict_source, strict_sink: universe.strict_sink};
}

export const transportLabel = (kind: "rocket-export" | "orbital-drop") => kind === "rocket-export" ? "火箭发运" : "轨道投送";
