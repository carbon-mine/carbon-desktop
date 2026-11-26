// SPDX-License-Identifier: MPL-2.0
// Run from this directory after npm ci. Chrome is needed only to bake the native assets.
import { build } from 'esbuild';
import { chromium } from 'playwright-core';
import { createHash } from 'node:crypto';
import { mkdir, writeFile, rename, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const manifest = JSON.parse(await readFile(new URL('../../design-system/landscapes/manifest.json', import.meta.url), 'utf8'));
const framesPerView = 48;
const output = fileURLToPath(new URL('../../design-system/landscapes/', import.meta.url));
const assets = await build({
  entryPoints: [fileURLToPath(new URL('live.js', import.meta.url))],
  bundle: true, write: false, format: 'esm', minify: true,
  loader: { '.hdr': 'dataurl', '.png': 'dataurl' },
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
    if (!entry.frames) continue;
    const view = entry.id;
    await page.goto(base + (view.endsWith('-overhead') ? '?overhead' : '?scenic') + '#' + view.replace('-overhead', ''));
    await page.waitForFunction(() => typeof window.paintScenicFrame === 'function');
    const frames = [];
    for (let index = 0; index < framesPerView; index++) {
      const jpg = await page.evaluate(fraction => {
        window.paintScenicFrame(fraction);
        return document.querySelector('canvas').toDataURL('image/jpeg', 0.81).split(',')[1];
      }, index / (framesPerView - 1));
      frames.push(Buffer.from(jpg, 'base64'));
    }
    const distinct = new Set(frames.map(frame => createHash('sha256').update(frame).digest('hex')));
    if (distinct.size < framesPerView * 0.9 || errors.length) throw Error(view + ' did not render: ' + errors.join('; '));
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
    await writeFile(output + '/' + entry.poster, frames[0]);
    await writeFile(dest + '.next', packed);
    await rename(dest + '.next', dest);
    digest.update(packed);
    console.log(view, packed.length, 'bytes', distinct.size, 'different camera frames');
  }
  console.log('Pack set SHA-256', digest.digest('hex'));
} finally {
  await browser.close();
}
