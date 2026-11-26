// SPDX-License-Identifier: MPL-2.0
// Inspect complete encoded cycles, including the transition across a seek.
import { spawn } from 'node:child_process';

const width=384, height=216, channels=3, length=width*height*channels;
const xBlocks=16, yBlocks=12;
function difference(left,right) {
  const blocks=new Float64Array(xBlocks*yBlocks);
  let total=0;
  for(let y=0;y<height;y++){
    const row=Math.floor(y*yBlocks/height)*xBlocks;
    for(let x=0;x<width;x++){
      const offset=(y*width+x)*channels;
      const amount=Math.abs(left[offset]-right[offset])
        +Math.abs(left[offset+1]-right[offset+1])+Math.abs(left[offset+2]-right[offset+2]);
      total+=amount;
      blocks[row+Math.floor(x*xBlocks/width)]+=amount;
    }
  }
  return {mean:total/length, maxBlock:Math.max(...blocks)/(length/blocks.length)};
}

async function inspect(file) {
  const child=spawn(process.env.FFMPEG_PATH??'ffmpeg',['-v','error','-xerror','-i',file,
    '-vf',`scale=${width}:${height}:flags=area`,'-pix_fmt','rgb24','-f','rawvideo','pipe:1'],
    {stdio:['ignore','pipe','inherit'],windowsHide:true});
  const closed=new Promise((resolve,reject)=>{
    child.once('error',reject);
    child.once('close',resolve);
  });
  let first,prior,frame=Buffer.alloc(length),offset=0;
  const steps=[];
  for await(const chunk of child.stdout){
    for(let start=0;start<chunk.length;){
      const amount=Math.min(chunk.length-start,length-offset);
      chunk.copy(frame,offset,start,start+amount);
      start+=amount;offset+=amount;
      if(offset===length){
        if(prior) steps.push(difference(prior,frame));
        else first=frame;
        prior=frame;frame=Buffer.alloc(length);offset=0;
      }
    }
  }
  if(await closed!==0 || offset || !steps.length) throw Error('Incomplete decoded clip: '+file);
  const seam=difference(prior,first);
  const deltas=[seam,...steps].map(s=>s.mean);
  const sorted=[...deltas].sort((a,b)=>a-b);
  const p95=sorted[Math.ceil(.95*sorted.length)-1];
  return {file,frames:deltas.length,seam,previous:steps.at(-1),next:steps[0],p95,
    maximum:Math.max(...deltas),worstFrame:deltas.indexOf(Math.max(...deltas))};
}

const files=process.argv.slice(2);
if (!files.length) throw Error('Pass one or more local AVC clips');
for(const file of files){
  const result=await inspect(file);
  console.log(JSON.stringify(result));
  if(result.seam.mean>result.p95) process.exitCode=1;
}
