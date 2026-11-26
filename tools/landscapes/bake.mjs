// SPDX-License-Identifier: MPL-2.0
// Run from this directory after npm ci. Chrome is needed only to bake the native assets.
import { build } from 'esbuild';
import { authoringSize, cameraFor, validateOutputs } from './source-config.mjs';
import { exportAvc } from './avc-export.mjs';
import { chromium } from 'playwright-core';
import { createHash } from 'node:crypto';
import { mkdir, writeFile, rename, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import path from 'node:path';

const {values:options}=parseArgs({options:{
  'png-dir':{type:'string'}, 'avc-dir':{type:'string'}, view:{type:'string'}, size:{type:'string'}, frames:{type:'string'},
}});
const pngDir=options['png-dir'];
const avcDir=options['avc-dir'];
const exportDir=pngDir??avcDir;
if(pngDir && avcDir) throw Error('Choose one export format per run');
if(avcDir && options.frames) throw Error('AVC clips cover the complete authored cycle');
if(!exportDir && options.frames) throw Error('Use --png-dir for a partial source export');
const {width,height,pixels}=authoringSize(options.size??(avcDir ? '1440x810' : '960x540'));
const requestedFrames=Number(options.frames??1);
if(!Number.isSafeInteger(requestedFrames) || requestedFrames<1) throw Error('--frames must be a positive integer');

const manifest = JSON.parse(await readFile(new URL('../../design-system/landscapes/manifest.json', import.meta.url), 'utf8'));
validateOutputs(manifest.views);
if(options.view && !manifest.views.some(view=>view.id===options.view && view.bake)) throw Error('Unknown authored view '+options.view);
if (!manifest.views.some(entry => entry.bake)) process.exit(0);
const sources = JSON.parse(await readFile(new URL('sources.json', import.meta.url), 'utf8'));
for (const entry of manifest.views) if (entry.bake) cameraFor(sources, entry.bake);
const cycleSeconds = 8;
const output = fileURLToPath(new URL('../../design-system/landscapes/', import.meta.url));
const assets = await build({
  entryPoints: [fileURLToPath(new URL('live.js', import.meta.url))],
  bundle: true, write: false, format: 'esm', minify: true,
  loader: { '.hdr': 'dataurl', '.png': 'dataurl' },
  plugins: [{
    name: 'local-landscape-geometry',
    setup(builder) {
      builder.onResolve({filter: /^carbon:geometry$/}, () => ({path:'geometry', namespace:'carbon'}));
      builder.onLoad({filter: /.*/,namespace:'carbon'}, () => {
        const imports = [], entries = [];
        Object.entries(sources).forEach(([id, source], index) => {
          if (!/^[a-z0-9-]+\.js$/.test(source.module) || !/^[a-zA-Z_$][a-zA-Z0-9_$]*$/.test(source.factory)) throw Error('Invalid local geometry source ' + id);
          imports.push('import {' + source.factory + ' as factory' + index + '} from ' + JSON.stringify('./'+source.module) + ';');
          entries.push(JSON.stringify(id) + ':factory' + index);
        });
        return {contents: imports.join('\n')+'\nexport const geometry={'+entries.join(',')+'};', resolveDir:fileURLToPath(new URL('.',import.meta.url))};
      });
    }
  }],
});
const base = 'http://landscape.invalid/';
const script = assets.outputFiles[0].text.replaceAll('</script', '<\\/script');
const html = '<style>*{margin:0}canvas{display:block;width:100vw;height:100vh}button,[role=status]{display:none}</style><canvas></canvas><button></button><div role=status></div><script type=module>' + script + '</script>';
const browser = await chromium.launch(process.env.CHROME_PATH
  ? { executablePath: process.env.CHROME_PATH }
  : { channel: 'chrome' });
try {
  await mkdir(output, { recursive: true });
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.route(/^https?:/, route => route.request().url().startsWith(base)
    ? route.fulfill({ contentType: 'text/html', body: html })
    : route.abort());
  const digest = createHash('sha256');
  for (const entry of manifest.views) {
    if (!entry.bake || (options.view && options.view!==entry.id)) continue;
    const view = entry.id;
    await page.goto(base + '?' + new URLSearchParams({...entry.bake, pixels:String(pixels)}));
    await page.waitForFunction(() => typeof window.paintScenicFrame === 'function');
    const canvas = await page.evaluate(() => {
      const element = document.querySelector('canvas');
      return [element.width, element.height];
    });
    if(canvas[0]!==width || canvas[1]!==height) {
      throw Error(view+' rendered a '+canvas.join('x')+' canvas instead of '+width+'x'+height);
    }
    const framesPerView = entry.frames ? entry.fps * cycleSeconds : 1;
    if(avcDir){
      if(!entry.frames) throw Error('AVC export needs a motion frame rate for '+view);
      await exportAvc(page,errors,{directory:avcDir,view,width,height,fps:entry.fps,count:framesPerView});
      console.log(view,framesPerView,'original source frames encoded as AVC');
      continue;
    }
    if(pngDir){
      if(requestedFrames>framesPerView) throw Error('Requested frames exceed the source cycle of '+view);
      if(!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(view)) throw Error('Invalid source view ID');
      const viewDir=path.join(exportDir,view);
      await mkdir(viewDir,{recursive:true});
      for(let index=0;index<requestedFrames;index++){
        const raw=await page.evaluate(phase=>{
          window.paintScenicFrame(phase);
          return document.querySelector('canvas').toDataURL('image/png').split(',')[1];
        },index/framesPerView);
        if(errors.length) throw Error(view+' source render failed: '+errors.join('; '));
        const name=String(index).padStart(3,'0')+'.png';
        await writeFile(path.join(viewDir,name),Buffer.from(raw,'base64'),{flag:'wx'});
      }
      console.log(view,requestedFrames,'lossless source frames',width,height);
      continue;
    }
    const frames = [];
    for (let index = 0; index < (entry.frames ? framesPerView : 1); index++) {
      const jpg = await page.evaluate(fraction => {
        window.paintScenicFrame(fraction);
        return document.querySelector('canvas').toDataURL('image/jpeg', 0.62).split(',')[1];
      }, index / framesPerView);
      frames.push(Buffer.from(jpg, 'base64'));
    }
    if (entry.frames) {
      const end = await page.evaluate(() => {
        window.paintScenicFrame(1);
        return document.querySelector('canvas').toDataURL('image/jpeg', 0.62).split(',')[1];
      });
      if (!frames[0].equals(Buffer.from(end, 'base64'))) throw Error(view + ' cycle does not return to its opening pose');
    }
    if (errors.length || frames.some(bytes => bytes.length < 4 || bytes.readUInt16BE(0) !== 0xffd8 || bytes.readUInt16BE(bytes.length - 2) !== 0xffd9)) {
      throw Error(view + ' produced invalid image data or page errors: ' + errors.join('; '));
    }
    const poster = output + '/' + entry.poster;
    if (!entry.frames) {
      await writeFile(poster + '.next', frames[0]);
      await rename(poster + '.next', poster);
      continue;
    }
    const distinct = new Set(frames.map(frame => createHash('sha256').update(frame).digest('hex')));
    if (distinct.size < framesPerView * 0.9) throw Error(view + ' produced too few distinct frames');
    const offsets = Buffer.alloc(4 * (frames.length + 2));
    offsets.writeUInt32LE(frames.length, 0);
    let length = 0;
    frames.forEach((frame, i) => {
      offsets.writeUInt32LE(length, 4 * (1 + i));
      length += frame.length;
    });
    offsets.writeUInt32LE(length, offsets.length - 4);
    const packed = Buffer.concat([offsets, ...frames]);
    const dest = output + '/' + entry.frames;
    await writeFile(dest + '.next', packed);
    await writeFile(poster + '.next', frames[0]);
    await rename(dest + '.next', dest);
    await rename(poster + '.next', poster);
    digest.update(packed);
    console.log(view, packed.length, 'bytes', distinct.size, 'different camera frames');
  }
  if(!exportDir) console.log('Pack set SHA-256', digest.digest('hex'));
} finally {
  await browser.close();
}
