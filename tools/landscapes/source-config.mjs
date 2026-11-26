// SPDX-License-Identifier: MPL-2.0
export function cameraFor(sources, reference) {
  if (!Object.hasOwn(sources, reference.source)) throw Error('Unknown authored scene: '+reference.source);
  const cameras=sources[reference.source].cameras;
  if (!Object.hasOwn(cameras, reference.camera)) throw Error('Unknown authored camera: '+reference.camera);
  const camera=cameras[reference.camera];
  if (![camera.eye,camera.target].every(v=>Array.isArray(v)&&v.length===3&&v.every(Number.isFinite))) throw Error('Invalid camera vectors');
  return camera;
}

export function validateOutputs(views) {
  const outputs = new Set();
  for (const view of views.filter(view => !view.bake)) {
    for (const name of [view.poster, view.frames].filter(Boolean)) {
      outputs.add(name.toLowerCase());
    }
  }
  for (const view of views.filter(view => view.bake)) {
    const files = [[view.poster, /\.jpe?g$/i]];
    if (view.frames) files.push([view.frames, /\.frames$/]);
    for (const [name, extension] of files) {
      if (typeof name !== 'string' || !/^[a-z0-9][a-z0-9_.-]+$/i.test(name) || !extension.test(name)) {
        throw Error('Invalid local output name for ' + view.id);
      }
      const key = name.toLowerCase();
      if (outputs.has(key) || outputs.has(key + '.next')) {
        throw Error('Shared output would overwrite another bundled view: ' + name);
      }
      outputs.add(key);
    }
  }
}
