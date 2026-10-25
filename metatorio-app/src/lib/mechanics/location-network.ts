import {locationKey, locationLabel} from './located.ts';
import {networkFlowKey, networkRate, type NetworkModel, type NetworkEdge} from './network.ts';
import {presentFlow} from '../dual-var.ts';
import type {Localize} from '../display-labels.ts';
import type {TimeScale, Location} from '../runtime/types';

/** Display projection only: transport crosses the remote location's boundary.
 * Keep the flow's real domain even if collapse joined two ground mechanisms. */
export function clusterTransportNetwork(model:NetworkModel, localize:Localize, timeScale:TimeScale):NetworkModel {
  const nodes=new Map(model.nodes.map(node=>[node.id,node]));
  const locations=new Map<string,Location>();
  for(const node of nodes.values()) if(node.location) locations.set(locationKey(node.location),node.location);
  const detached=new Set<string>();
  const edges:NetworkEdge[]=[];
  for(const edge of model.edges) {
    const source=nodes.get(edge.source),target=nodes.get(edge.target);
    const location=edge.flowLocation;
    const remote=(node:typeof source)=>!!location && node?.kind==='mechanic'
      && node.entries.some(entry=>entry.transport==='rocket-export'||entry.transport==='orbital-drop')
      && locationKey(node.location)!==locationKey(location);
    const from=remote(source),to=remote(target);
    if(!edge.flow || typeof edge.flow==='string' || !('Item' in edge.flow) || !location || (!from&&!to)) {
      edges.push(edge);continue;
    }
    locations.set(locationKey(location),location);
    const group=`location:${locationKey(location)}`;
    const info=presentFlow(edge.flow,localize);
    const label=[info.label,...info.qualifiers].join(' · ');
    const contributionsFor=(node:typeof source)=>edge.contributions.filter(c=>node?.entries.some(e=>e.id===c.mechanic));
    const transport=(node:typeof source,outgoing:boolean)=>{
      const contributions=contributionsFor(node);
      const amount=model.solved
        ? networkRate(edge.flow!,contributions.reduce((sum,c)=>sum+Math.abs(c.amount),0),timeScale)
        : `单位 ${[...new Set(contributions.map(c=>networkRate(edge.flow!,c.unitAmount,timeScale)))].join(' / ')}`;
      edges.push({...edge,id:`${edge.id}:transport:${outgoing?'out':'in'}`,kind:'transport',
        source:outgoing?node!.id:group,target:outgoing?group:node!.id,contributions,label:`${label}\n${amount}`});
    };
    if(from) transport(source,true);
    if(to) transport(target,false);
    if(edge.kind==='collapsed') {
      // Both transports terminate at the common domain, not at each other's
      // ground location. With a local space recipe, restore only its local leg.
      if(!from||!to) {
        const id=`f:${locationKey(location)}|${networkFlowKey(edge.flow)}`;
        if(!nodes.has(id)) nodes.set(id,{id,kind:'flow',flow:edge.flow,location,entries:[],
          label:[info.label,...info.qualifiers].join('\n'),
          search:`${locationLabel(location,localize)} ${info.description} ${networkFlowKey(edge.flow)}`,utility:edge.utility});
        const local=from?target:source;
        const contributions=contributionsFor(local);
        const amount=model.solved
          ? networkRate(edge.flow,contributions.reduce((sum,c)=>sum+Math.abs(c.amount),0),timeScale)
          : `单位 ${[...new Set(contributions.map(c=>networkRate(edge.flow!,c.unitAmount,timeScale)))].join(' / ')}`;
        edges.push({...edge,id:`${edge.id}:local`,kind:'flow',source:from?id:source!.id,
          target:from?target!.id:id,contributions,label:amount});
      }
    } else {
      const flow=from?target:source;
      if(flow?.kind==='flow') detached.add(flow.id);
    }
  }
  const connected=new Set(edges.flatMap(edge=>[edge.source,edge.target]));
  for(const id of detached) if(!connected.has(id)&&!nodes.get(id)?.target) nodes.delete(id);
  return {...model,nodes:[...nodes.values()],edges,locations:[...locations.values()]};
}
