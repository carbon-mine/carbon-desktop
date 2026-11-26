// SPDX-License-Identifier: MPL-2.0
import { spawn, execFile } from 'node:child_process';
import { once } from 'node:events';
import { constants } from 'node:fs';
import { copyFile, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { promisify } from 'node:util';

const run = promisify(execFile);

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
    '-g',String(fps),'-keyint_min',String(fps),'-sc_threshold','0','-color_range','tv',
    '-colorspace','bt709','-color_primaries','bt709','-color_trc','iec61966-2-1',
    '-x264-params','colorprim=bt709:transfer=iec61966-2-1:colormatrix=bt709',
    '-movflags','+faststart+write_colr',movie];
  const encoder=spawn(process.env.FFMPEG_PATH??'ffmpeg',args,{stdio:['pipe','ignore','pipe'],windowsHide:true});
  let diagnostic='';
  encoder.stderr.on('data',chunk=>{diagnostic=(diagnostic+chunk.toString()).slice(-4096);});
  const published=[];
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
    const [,[code]]=await Promise.all([pipeline(Readable.from(originals()),encoder.stdin),once(encoder,'close')]);
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
    for(const target of published) await rm(target,{force:true});
    throw error;
  } finally {
    if(encoder.exitCode===null) encoder.kill();
    await rm(stage,{recursive:true,force:true});
  }
}
