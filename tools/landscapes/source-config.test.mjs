// SPDX-License-Identifier: MPL-2.0
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { cameraFor } from './source-config.mjs';
const sources = JSON.parse(await readFile(new URL('sources.json', import.meta.url), 'utf8'));
test('a public view ID is independent of the authored camera', () => {
  const renamed = { id: 'market-square', bake: { source: 'farmland', camera: 'overhead' } };
  assert.deepEqual(cameraFor(sources, renamed.bake).eye, [1,32,8]);
});
test('unresolved geometry and undeclared angles are rejected', () => {
  assert.throws(() => cameraFor(sources,{source:'missing',camera:'scenic'}), /Unknown authored scene/);
  assert.throws(() => cameraFor(sources,{source:'alpine',camera:'overhead'}), /Unknown authored camera/);
});
