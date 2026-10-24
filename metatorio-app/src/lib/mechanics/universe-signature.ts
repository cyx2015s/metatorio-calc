import type {AppDocument} from '../runtime/types';

const ordered=<T extends {id:number}>(items:T[])=>[...items].sort((a,b)=>a.id-b.id);
/** Numerical input identity: names, ordering and display time units do not invalidate a solve. */
export function universeSolveSignatures(document:AppDocument|null):Map<string,string>{
  const signatures=new Map<string,string>();
  for(const project of document?.projects??[]){
    const {time_scale,...settings}=project.settings;
    for(const universe of project.universes??[]){
      const numerical={
        strict_source:universe.strict_source,strict_sink:universe.strict_sink,
        mechanics:ordered(universe.mechanics),targets:ordered(universe.targets),external_inputs:ordered(universe.external_inputs??[]),
        target_expressions:ordered(universe.target_expressions).map(e=>({...e,terms:ordered(e.terms)})),
      };
      signatures.set(`${project.id}:${universe.id}`,JSON.stringify([project.context_id,settings,numerical]));
    }
  }
  return signatures;
}
