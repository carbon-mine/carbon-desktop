# Native Home landscapes

Carbon's Home scenery depicts five authored voxel scenes. Riverside Mill and Dewfall Farm also have overhead camera compositions. Alpine Lake, Willow Marsh and Cider Orchard have scenic views. The checked-in images depict these demonstration landscapes. The launcher uses a separate data source for a player's world map.

Each view includes 48 actual 960 × 540 perspective frames along a 0.065-radian (3.72°) level camera arc. Its first frame is its poster. The bake renders original geometry, depth, directional lighting, foliage and water reflection at every camera position. Actors stay in a fixed pose as the camera travels. The eased path can be traversed in both directions without an abrupt jump at its ends. The seven packaged sequences total 26,924,500 bytes.

## Rebuilding the scenery

Install Chrome and Node.js, then run the following from `tools/landscapes`. Set `CHROME_PATH` to select a particular Chromium executable.

```sh
npm ci
npm run bake
```

The scene sources, original voxel models, camera definitions, sky lighting and siding material live in this directory. All geometry is defined in source. The baker runs offline after package installation, with all assets inlined. It rejects page errors and insufficient variation between frames. The observed Windows bake used Chrome 158.0.8127.88 and produced 48 distinct images per view. Hardware, browser and encoder versions can affect rendered bytes. [Materials, source links and reuse terms](../../tools/landscapes/assets/ASSETS.md)

Each `.frames` file starts with a little-endian 32-bit frame count followed by `count + 1` little-endian 32-bit offsets relative to the first byte of the JPEG payload. Each successive pair defines one independent image, including the terminal offset. Playback needs only the current frame and one decoded successor. JPEGs are decoded on demand. The desktop does not need Chrome, Three.js or an online service to show these scenes.
