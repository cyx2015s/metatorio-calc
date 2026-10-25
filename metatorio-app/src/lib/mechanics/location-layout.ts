import {locationKey} from './located.ts';
import type {Location} from '../runtime/types';
import type {NetworkModel} from './network';
import type {ElkNode} from 'elkjs/lib/elk-api';

export const locationLayoutOptions = {
  'elk.algorithm': 'layered', 'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
  'elk.spacing.nodeNode': '45', 'elk.layered.spacing.nodeNodeBetweenLayers': '100',
  'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
};
export const nodeLayoutSize = (node: NetworkModel['nodes'][number]) => ({id:node.id,
  width:node.kind==='mechanic'?145:115,
  height:Math.max(100,(node.kind==='mechanic'?48:38)*2+16+node.label.split('\n').length*26)});

/** Lay out each location, then arrange whole locations using their visible edges. */
export async function layoutLocations(model:NetworkModel, layout:(graph:ElkNode)=>Promise<ElkNode>) {
  const groups=new Map<string,{id:string;location:Location;nodes:typeof model.nodes}>();
  for(const node of model.nodes) {
    if(!node.location) continue;
    const key=locationKey(node.location);
    if(!groups.has(key)) groups.set(key,{id:`location:${key}`,location:node.location,nodes:[]});
    groups.get(key)!.nodes.push(node);
  }
  const owner=new Map<string,string>();
  for(const group of groups.values()) for(const node of group.nodes) owner.set(node.id,group.id);
  const layouts=await Promise.all([...groups.values()].map(async group=>{
    const graph=await layout({id:group.id,layoutOptions:locationLayoutOptions,
      children:group.nodes.map(nodeLayoutSize),
      edges:model.edges.filter(e=>owner.get(e.source)===group.id&&owner.get(e.target)===group.id)
        .map(e=>({id:e.id,sources:[e.source],targets:[e.target]}))});
    return {...group,graph};
  }));
  const links=new Map<string,{id:string;sources:string[];targets:string[]}>();
  for(const edge of model.edges) {
    const from=owner.get(edge.source),to=owner.get(edge.target);
    if(from&&to&&from!==to) links.set(`${from}|${to}`,{id:`cluster:${from}|${to}`,sources:[from],targets:[to]});
  }
  const arranged=await layout({id:'locations',layoutOptions:{...locationLayoutOptions,'elk.spacing.nodeNode':'100'},
    children:layouts.map(g=>({id:g.id,width:(g.graph.width??200)+80,height:(g.graph.height??120)+100})),edges:[...links.values()]});
  const offsets=new Map(arranged.children?.map(g=>[g.id,{x:(g.x??0)+40,y:(g.y??0)+60}]));
  const positions=new Map<string,{x:number;y:number}>();
  for(const group of layouts) {
    const offset=offsets.get(group.id)!;
    for(const node of group.graph.children??[]) positions.set(node.id,{x:offset.x+(node.x??0)+75,y:offset.y+(node.y??0)+45});
  }
  return {positions,groups:[...groups.values()],owner};
}
