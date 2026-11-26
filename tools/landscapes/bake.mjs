// SPDX-License-Identifier: MPL-2.0
// Run from this directory after npm ci. Chrome is needed only to bake the native assets.
import { build } from 'esbuild';
import { cameraFor } from './source-config.mjs';
import { chromium } from 'playwright-core';
import { createHash } from 'node:crypto';
import { mkdir, writeFile, rename, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const manifest = JSON.parse(await readFile(new URL('../../design-system/landscapes/manifest.json', import.meta.url), 'utf8'));
if (!manifest.views.some(entry => entry.bake)) process.exit(0);
const sources = JSON.parse(await readFile(new URL('sources.json', import.meta.url), 'utf8'));
for (const entry of manifest.views) if (entry.bake) cameraFor(sources, entry.bake);
const framesPerView = 48;
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
  const page = await browser.newPage({ viewport: { width: 960, height: 540 }, deviceScaleFactor: 1, reducedMotion: 'reduce' });
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.route(/^https?:/, route => route.request().url().startsWith(base)
    ? route.fulfill({ contentType: 'text/html', body: html })
    : route.abort());
  const digest = createHash('sha256');
  for (const entry of manifest.views) {
    if (!entry.bake) continue;
    const view = entry.id;
    await page.goto(base + '?' + new URLSearchParams(entry.bake));
    await page.waitForFunction(() => typeof window.paintScenicFrame === 'function');
    const frames = [];
    for (let index = 0; index < (entry.frames ? framesPerView : 1); index++) {
      const jpg = await page.evaluate(fraction => {
        window.paintScenicFrame(fraction);
        return document.querySelector('canvas').toDataURL('image/jpeg', 0.81).split(',')[1];
      }, index / (framesPerView - 1));
      frames.push(Buffer.from(jpg, 'base64'));
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
  console.log('Pack set SHA-256', digest.digest('hex'));
} finally {
  await browser.close();
}
