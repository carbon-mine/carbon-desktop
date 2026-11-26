// SPDX-License-Identifier: MPL-2.0
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, readdir, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { exportAvc, loopQualityZones } from './avc-export.mjs';

const available = spawnSync(process.env.FFMPEG_PATH ?? 'ffmpeg', ['-version'], {windowsHide:true}).status === 0;

test('an interrupted source render retires its encoder before removing stage files', { skip: !available }, async () => {
  const dir = await mkdtemp(path.join(tmpdir(), 'carbon-avc-interrupt-'));
  let frames = 0;
  let observedMovie = false;
  const page = { async evaluate() {
    frames++;
    const [stage] = await readdir(dir);
    if (stage && await stat(path.join(dir,stage,'test-view.mp4')).catch(()=>null)) {
      observedMovie = true;
      throw Error('source renderer interrupted');
    }
    await new Promise(resolve=>setTimeout(resolve,5));
    const png = 'iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAE0lEQVR4nGNkaGDACpiwCw9WCQBqCACQJ5at+QAAAABJRU5ErkJggg==';
    return {png, poster:png};
  }};
  try {
    await assert.rejects(exportAvc(page, [], {directory:dir,view:'test-view',width:8,height:8,fps:30,count:240}), /source renderer interrupted/);
    assert.ok(frames > 1 && observedMovie);
    assert.deepEqual(await readdir(dir),[]);
  } finally {
    await rm(dir,{recursive:true,force:true});
  }
});

test('encoding precision changes continuously around the complete loop', () => {
  const zones=loopQualityZones(240,30).split('/');
  const qps=[];
  for (const zone of zones) {
    const [,start,end,qp]=zone.match(/^(\d+),(\d+),q=(\d+)$/).map(Number);
    assert.equal(start,qps.length);
    for (let i=start;i<=end;i++) qps.push(qp);
  }
  assert.equal(qps.length,240);
  assert.deepEqual(qps,[...qps].reverse());
  assert.equal(qps[0],1);
  assert.equal(qps[120],24);
  assert.ok(qps.every((qp,i)=>i===0 || Math.abs(qp-qps[i-1])<=1));
});
