// SPDX-License-Identifier: MPL-2.0
// 8,294,400 pixels (3840 x 2160) bounds the authoring browser's GPU work, and
// no edge may exceed the 4096-pixel texture size common to authoring hardware.
export const MAX_AUTHORING_PIXELS = 8_294_400;
export const MAX_AUTHORING_EDGE = 4_096;

export function authoringSize(text) {
  const match = /^([1-9][0-9]{0,4})x([1-9][0-9]{0,4})$/.exec(text ?? '');
  if (!match) throw Error('Use a width and height such as 2560x1440');
  const width = Number(match[1]);
  const height = Number(match[2]);
  if (width < 2 || height < 2) throw Error('Use at least 2x2 pixels');
  if (width * height > MAX_AUTHORING_PIXELS) {
    throw Error('Source size exceeds the ' + MAX_AUTHORING_PIXELS + '-pixel authoring budget');
  }
  if (width > MAX_AUTHORING_EDGE || height > MAX_AUTHORING_EDGE) {
    throw Error('Source dimension exceeds the ' + MAX_AUTHORING_EDGE + '-pixel authoring edge');
  }
  return { width, height, pixels: width * height };
}

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
