const {spawn}=require('node:child_process');
const {createInterface}=require('node:readline');
function createPngNativeBridge(executable){
 const process=spawn(executable,[],{windowsHide:true,stdio:['pipe','pipe','inherit']});
 const pending=[];
 createInterface({input:process.stdout}).on('line',line=>{
  const request=pending.shift();if(!request)return;
  try{const reply=JSON.parse(line);reply.error?request.reject(Error(reply.error)):request.resolve(reply.ok);}catch(e){request.reject(e);}
 });
 process.on('error',e=>{while(pending.length)pending.shift().reject(e)});
 process.on('exit',code=>{while(pending.length)pending.shift().reject(Error(`PNG bridge exited: ${code}`))});
 return {call(request){return new Promise((resolve,reject)=>{pending.push({resolve,reject});process.stdin.write(JSON.stringify(request)+'\n');});},close(){process.stdin.end();}};
}
module.exports={createPngNativeBridge};
