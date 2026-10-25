// Browser-only protocol mock for the small UI regressions. The large real-project
// regression uses network-png-bridge and the actual Rust streaming encoder.
function installNetworkPngMock({deferDialog=false}={}) {
 const raw=window.__TAURI_INTERNALS__.invoke,sessions=new Map();let next=1;
 window.__pngExports=[];window.__pngAborts=0;
 window.__TAURI_INTERNALS__.invoke=async(command,args={})=>{
  if(command==='begin_network_png') {
   const record={...args,bytes:[]};window.__pngExports.push(record);
   let mode='success';
   if(deferDialog)mode=await new Promise(resolve=>window.__finishPngSave=value=>{window.__finishPngSave=null;resolve(value)});
   if(mode==='cancel')return null;
   const canvas=document.createElement('canvas');canvas.width=args.width;canvas.height=args.height;
   const id=next++;sessions.set(id,{canvas,record,fail:mode==='error'});
   return {id,width:args.width,height:args.height,tile_width:1024,tile_height:128};
  }
  if(command==='write_network_png_tile') {
   if(window.__failPngTile)throw Error('模拟分区传输失败');
   if(window.__holdPngTile)await new Promise(resolve=>window.__releasePngTile=resolve);
   const state=sessions.get(args.exportId);if(!state)throw Error('missing export');
   const image=new Image();image.src=`data:image/png;base64,${args.imageBase64}`;await image.decode();
   state.canvas.getContext('2d').drawImage(image,args.x,args.y);return;
  }
  if(command==='finish_network_png') {
   const state=sessions.get(args.exportId);if(!state)throw Error('missing export');
   if(state.fail)throw Error('模拟磁盘写入失败');
   const blob=await new Promise(resolve=>state.canvas.toBlob(resolve));
   state.record.bytes=Array.from(new Uint8Array(await blob.arrayBuffer()).slice(0,24));
   state.canvas.width=state.canvas.height=0;sessions.delete(args.exportId);
   return `C:\\Exports\\${state.record.fileName}`;
  }
  if(command==='abort_network_png') {
   const state=sessions.get(args.exportId);if(state)state.canvas.width=state.canvas.height=0;
   sessions.delete(args.exportId);window.__pngAborts++;return;
  }
  return raw(command,args);
 };
}
module.exports={installNetworkPngMock};
