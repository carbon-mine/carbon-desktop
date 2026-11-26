// SPDX-License-Identifier: MPL-2.0
import { spawn, execFile } from 'node:child_process';
import { constants } from 'node:fs';
import { copyFile, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { promisify } from 'node:util';

const run = promisify(execFile);

// Independent and predicted frames quantize still textures differently. One
// IDR per loop and a gentle increase in precision around its seam keep the
// displayed first and last frames close without blending the moving geometry.
export function loopQualityZones(count, fps) {
  const ramp = Math.min(Math.round(fps * 1.5), Math.ceil(count / 2));
  const hold = Math.min(4, Math.floor(ramp / 4));
  const zones = [];
  for (let index = 0; index < count; index++) {
    const distance = Math.min(index, count - 1 - index);
    const amount = Math.min(1, Math.max(0, distance - hold) / Math.max(1, ramp - 1 - hold));
    const qp = Math.round(1 + 23 * amount);
    const previous = zones.at(-1);
    if (previous && previous.end + 1 === index && previous.qp === qp) previous.end = index;
    else zones.push({ start:index, end:index, qp });
  }
  return zones.map(({start,end,qp}) => `${start},${end},q=${qp}`).join('/');
}

// FFmpeg and Chromium are offline authoring dependencies. The native launcher
// reads the two finished assets, so neither tool runs on the player's machine.
export async function exportAvc(page, errors, { directory, view, width, height, fps, count }) {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(view) || width%2 || height%2) {
    throw Error('AVC export requires a stable view ID and even pixel dimensions');
  }
  await mkdir(directory, { recursive:true });
  const stage = await mkdtemp(path.join(directory, '.scenery-'));
  const movie = path.join(stage, view+'.mp4');
  const poster = path.join(stage, view+'.jpg');
  const filter = 'scale=in_range=full:out_range=limited:out_color_matrix=bt709,format=yuv420p,'
    +'setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709';
  const args = ['-hide_banner','-loglevel','error','-y','-f','image2pipe','-framerate',String(fps),
    '-vcodec','png','-i','pipe:0','-map','0:v:0','-an','-sn','-dn','-vf',filter,
    '-c:v','libx264','-profile:v','main','-preset','medium','-crf','17','-bf','0',
    '-g',String(count),'-keyint_min',String(count),'-sc_threshold','0','-color_range','tv',
    '-colorspace','bt709','-color_primaries','bt709','-color_trc','iec61966-2-1',
    '-x264-params','colorprim=bt709:transfer=iec61966-2-1:colormatrix=bt709:zones='+loopQualityZones(count,fps),
    '-movflags','+faststart+write_colr',movie];
  const encoder=spawn(process.env.FFMPEG_PATH??'ffmpeg',args,{stdio:['pipe','ignore','pipe'],windowsHide:true});
  let diagnostic='';
  encoder.stderr.on('data',chunk=>{diagnostic=(diagnostic+chunk.toString()).slice(-4096);});
  let processError;
  encoder.once('error',error=>{ processError=error; });
  const closed=new Promise(resolve=>encoder.once('close',resolve));
  const published=[];
  let failure;
  try {
    async function* originals(){
      let first;
      for(let index=0;index<=count;index++){
        const sample=await page.evaluate(({phase,opening})=>{
          window.paintScenicFrame(phase);
          const canvas=document.querySelector('canvas');
          // Both encodings read the same drawing buffer before Chromium clears it.
          return {png:canvas.toDataURL('image/png').split(',')[1],
            poster:opening ? canvas.toDataURL('image/jpeg',.94).split(',')[1] : null};
        },{phase:index/count,opening:index===0});
        if(errors.length) throw Error(view+' source render failed: '+errors.join('; '));
        const png=Buffer.from(sample.png,'base64');
        if(index===0){
          first=png;
          await writeFile(poster,Buffer.from(sample.poster,'base64'));
        }
        if(index===count){
          if(!first.equals(png)) throw Error(view+' does not close its authored loop');
          return;
        }
        yield png;
      }
    }
    const [,code]=await Promise.all([pipeline(Readable.from(originals()),encoder.stdin),closed]);
    if(processError) throw processError;
    if(code!==0) throw Error('FFmpeg failed for '+view+': '+diagnostic);
    const {stdout}=await run(process.env.FFPROBE_PATH??'ffprobe',['-v','error','-show_streams','-of','json',movie],{windowsHide:true});
    const streams=JSON.parse(stdout).streams, s=streams[0];
    if(streams.length!==1 || s.codec_name!=='h264' || s.pix_fmt!=='yuv420p' || s.width!==width || s.height!==height ||
      Number(s.nb_frames)!==count || s.r_frame_rate!==fps+'/1' || s.color_range!=='tv' ||
      s.color_space!=='bt709' || s.color_primaries!=='bt709' || s.color_transfer!=='iec61966-2-1') {
      throw Error('Unexpected encoded video properties for '+view);
    }
    for(const file of [poster,movie]){
      const target=path.join(directory,path.basename(file));
      await copyFile(file,target,constants.COPYFILE_EXCL);
      published.push(target);
    }
  } catch(error) {
    failure=error;
    throw error;
  } finally {
    if (encoder.pid && encoder.exitCode===null && encoder.signalCode===null) encoder.kill();
    await closed;
    try {
      if (failure) for (const target of published) await rm(target,{force:true});
      await rm(stage,{recursive:true,force:true});
    } catch (cleanupError) {
      if (failure) throw new AggregateError([failure,cleanupError],failure.message);
      throw cleanupError;
    }
  }
}
