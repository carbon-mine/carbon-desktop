// SPDX-License-Identifier: MPL-2.0
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import {addClouds} from './clouds.js';
import {addFantasy} from './fantasy.js';
import { cameraFor, validateOutputs } from './source-config.mjs';
const sources = JSON.parse(await readFile(new URL('sources.json', import.meta.url), 'utf8'));
test('a public view ID is independent of the authored camera', () => {
  const renamed = { id: 'market-square', bake: { source: 'farmland', camera: 'overhead' } };
  assert.deepEqual(cameraFor(sources, renamed.bake).eye, [1,32,8]);
});
test('unresolved geometry and undeclared angles are rejected', () => {
  assert.throws(() => cameraFor(sources,{source:'missing',camera:'scenic'}), /Unknown authored scene/);
  assert.throws(() => cameraFor(sources,{source:'alpine',camera:'overhead'}), /Unknown authored camera/);
});

test('authoring cannot write through relative paths or replace a supplied pack', () => {
  const authored = {id:'farm', poster:'farm.jpg', frames:'farm.frames', bake:{}};
  assert.doesNotThrow(() => validateOutputs([authored]));
  for (const poster of ['../other.jpg', 'sub\\other.jpg', 'manifest.json']) {
    assert.throws(() => validateOutputs([{...authored, poster}]), /Invalid local output/);
  }
  assert.throws(() => validateOutputs([authored,{id:'supplied',poster:'FARM.JPG'}]), /Shared output/);
});

test('new landscapes can omit the bundled horizon props', () => {
  for (const decorate of [addClouds, addFantasy]) {
    const boxes = [];
    decorate((...args) => boxes.push(args), 'new-valley');
    assert.equal(boxes.length, 0);
    decorate((...args) => boxes.push(args), 'riverside');
    assert.ok(boxes.length > 0);
  }
});
