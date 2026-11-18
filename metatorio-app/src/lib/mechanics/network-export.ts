// 显式使用原始尺寸，避免 Cytoscape 隐式乘以设备像素比；渲染按小块分区而非整图大画布。
export function networkPngSize(width: number, height: number) {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    throw new Error('网络图像边界无效，请先重排布局');
  }
  return {width: Math.ceil(width), height: Math.ceil(height)};
}

export interface NetworkTile {x:number;y:number;width:number;height:number}
export function* networkPngTiles(width:number,height:number,tileWidth:number,tileHeight:number):Generator<NetworkTile>{
  if(![width,height,tileWidth,tileHeight].every(n=>Number.isSafeInteger(n)&&n>0)) throw new Error('PNG 分区尺寸无效');
  for(let y=0;y<height;y+=tileHeight) for(let x=0;x<width;x+=tileWidth) {
    yield {x,y,width:Math.min(tileWidth,width-x),height:Math.min(tileHeight,height-y)};
  }
}
