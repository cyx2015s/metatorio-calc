import type {Core, ElementDefinition} from 'cytoscape';
import {networkPngSize,type NetworkTile} from './network-export';

const nextFrame=()=>new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
// Canvas renderer APIs are implemented by the bundled Cytoscape version but
// absent from its type declarations.
type CanvasCore=Core&{renderer():unknown;renderTo(context:CanvasRenderingContext2D,zoom:number,pan:{x:number;y:number},pixelRatio:number):Core};
/** A detached, immutable rendering snapshot. Its viewport is moved per tile so
 * Cytoscape's viewport culling sees that tile, never the user's current viewport. */
export async function createNetworkPngSnapshot(source:Core,background:string,cancelled:()=>boolean){
  const cytoscape=(await import('cytoscape')).default;
  await document.fonts.ready;
  const host=document.createElement('div');
  Object.assign(host.style,{position:'fixed',left:'-100000px',top:'0',width:'256px',height:'256px',visibility:'hidden',pointerEvents:'none'});
  document.body.append(host);
  let graph:CanvasCore|undefined;
  const check=()=>{if(cancelled())throw new Error('已取消导出');};
  try {
    check();
    // Element JSON omits bypasses; copying computed styles preserves quality and
    // location badge layers as well as the current font and display scale.
    const elements=source.elements().map(ele=>({...ele.json(),style:ele.style()} as ElementDefinition));
    graph=cytoscape({container:host,elements,style:source.json().style,layout:{name:'preset'},
      zoom:1,minZoom:1,maxZoom:1,pixelRatio:1,textureOnViewport:false,hideEdgesOnViewport:false,motionBlur:false}) as CanvasCore;
    // Offscreen nodes are culled and never update :backgrounding. Populate the
    // snapshot renderer's image cache explicitly, including all badge layers.
    const renderer=graph.renderer() as unknown as {getCachedImage(url:string,crossOrigin:string,onLoad:()=>void):HTMLImageElement};
    const urls=new Set<string>();
    graph.nodes().forEach(node=>{
      const style=(node as unknown as {pstyle(name:string):{value:(string|null)[]}}).pstyle('background-image');
      for(const url of style.value)if(url&&url!=='none')urls.add(url);
    });
    await Promise.all([...urls].map(async url=>{
      await renderer.getCachedImage(url,'anonymous',()=>{}).decode();check();
    }));
    graph.forceRender();await nextFrame();
    const bounds=graph.elements().boundingBox(),size=networkPngSize(bounds.w,bounds.h);
    const canvas=document.createElement('canvas');
    return {...size,
      async render(tile:NetworkTile):Promise<Blob>{
        check();
        const bleed=4;
        host.style.width=`${tile.width+bleed*2}px`;host.style.height=`${tile.height+bleed*2}px`;
        graph!.resize();
        const pan={x:-bounds.x1-tile.x+bleed,y:-bounds.y1-tile.y+bleed};
        graph!.viewport({zoom:1,pan});
        canvas.width=tile.width+bleed*2;canvas.height=tile.height+bleed*2;
        const ctx=canvas.getContext('2d');if(!ctx)throw new Error('无法创建 PNG 分区画布');
        graph!.renderTo(ctx,1,pan,1);
        ctx.setTransform(1,0,0,1,0,0);ctx.globalCompositeOperation='destination-over';ctx.fillStyle=background;ctx.fillRect(0,0,canvas.width,canvas.height);
        const pixels=ctx.getImageData(bleed,bleed,tile.width,tile.height);
        canvas.width=tile.width;canvas.height=tile.height;ctx.putImageData(pixels,0,0);
        const blob=await new Promise<Blob>((resolve,reject)=>canvas.toBlob(value=>value?resolve(value):reject(new Error('PNG 分区编码失败')),'image/png'));
        check();return blob;
      },
      destroy(){graph?.destroy();canvas.width=canvas.height=0;host.remove();}
    };
  } catch(error) {graph?.destroy();host.remove();throw error;}
}
