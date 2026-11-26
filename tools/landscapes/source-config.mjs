// SPDX-License-Identifier: MPL-2.0
export function cameraFor(sources, reference) {
  if (!Object.hasOwn(sources, reference.source)) throw Error('Unknown authored scene: '+reference.source);
  const cameras=sources[reference.source].cameras;
  if (!Object.hasOwn(cameras, reference.camera)) throw Error('Unknown authored camera: '+reference.camera);
  const camera=cameras[reference.camera];
  if (![camera.eye,camera.target].every(v=>Array.isArray(v)&&v.length===3&&v.every(Number.isFinite))) throw Error('Invalid camera vectors');
  return camera;
}
