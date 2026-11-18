import type {Core, ElementDefinition} from 'cytoscape';
import {networkPngSize,type NetworkTile} from './network-export';

const nextFrame=()=>new Promise<void>(resolve=>requestAnimationFrame(()=>resolve()));
// 内置 Cytoscape 版本实现了这些 Canvas 渲染器接口，但类型声明中未列出。
type CanvasCore=Core&{renderer():unknown;renderTo(context:CanvasRenderingContext2D,zoom:number,pan:{x:number;y:number},pixelRatio:number):Core};
/** 独立且不可变的渲染快照：逐块移动自身视口，使 Cytoscape 视口裁剪只看当前图块，
 * 不受用户当前交互视口影响。 */
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
    // 元素 JSON 不含旁路样式；拷贝计算后样式以保留品质/地点徽标图层及当前字体和显示缩放。
    const elements=source.elements().map(ele=>({...ele.json(),style:ele.style()} as ElementDefinition));
    graph=cytoscape({container:host,elements,style:source.json().style,layout:{name:'preset'},
      zoom:1,minZoom:1,maxZoom:1,pixelRatio:1,textureOnViewport:false,hideEdgesOnViewport:false,motionBlur:false}) as CanvasCore;
    // 屏幕外节点会被裁剪且不会触发 :backgrounding；显式预热快照渲染器的图片缓存（含全部徽标层）。
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
