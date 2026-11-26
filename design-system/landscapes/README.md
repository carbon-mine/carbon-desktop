# Native Home landscapes

Carbon's Home scenery depicts five authored voxel scenes. Riverside Mill and Dewfall Farm also have overhead camera compositions. Alpine Lake, Willow Marsh and Cider Orchard have scenic views. The checked-in images depict these demonstration landscapes. The launcher uses a separate data source for a player's world map.

Each view contains 192 unique 960 × 540 frames at 24 fps. During an eight-second loop the level camera eases through a 0.024-radian (1.38°) arc and returns. Wind continues in the same direction at camera turns. Small peripheral clouds, an occasional leaf or twig, moving cloud shadows and a low-contrast sunward glow appear in all seven views. Where the scene contains them, the original models animate the mill wheel, windmill blades, crops, reeds, chimney smoke and water. The first frame is the poster. The seven frame packs together occupy 69,895,420 bytes.

The baker samples phase zero through the final frame before phase one. It renders phase one separately and compares it with the opening poster to check that the scene returns to its start. It stores the opening image once. The checked-in pack uses a JPEG quality of 0.62 and retains the full native image dimensions.

![Seven native Windows landscape samples](../../docs/images/native-home-landscape-motion.jpg)

## Windows motion qualification

The native 960 × 540 scene was shown through a GPUI window in the Windows x64 development build. A disposable Riverside Mill probe sampled changing window pixels for ten seconds per client size. A separate four-second interval measured the process CPU share of one logical core. The harness checked for commands every 75 ms. Sampling through Win32 PrintWindow had approximately 12–17 ms resolution, so a single observed interval includes capture timing. The following comparison used the same viewport and probe before and after the frame pacing, decoder and artwork changes.

| Client width | Baseline visible fps | Updated visible fps | Baseline frame interval p50 / p95 | Updated frame interval p50 / p95 |
| --- | ---: | ---: | --- | --- |
| 704 × 445 | 4.4 | 24.0 | 230 / 255 ms | 48 / 50 ms |
| 960 × 540 | 4.1 | 24.0 | 243 / 279 ms | 47 / 49 ms |
| 1368 × 720 | 4.0 | 24.0 | 243 / 291 ms | 42 / 54 ms |

The wide probe used 68% of one logical core before the update and 21% afterward. The compact probe measured 64% and 24%. All three paused views stayed pixel-stable, including after minimizing and restoring while paused. Resuming restarted the motion. These measurements cover the standalone Windows renderer in the development profile. The full Home shell and other operating systems require their own native observations.


## Rebuilding the scenery

Install Chrome and Node.js, then run the following from `tools/landscapes`. Set `CHROME_PATH` to select a particular Chromium executable.

```sh
npm ci
npm run bake
```

The scene sources, original voxel models, camera definitions, sky lighting and siding material live under `tools/landscapes`. All geometry is defined in source. The baker runs offline after package installation, with all assets inlined. It rejects page errors and insufficient variation between frames. The observed Windows bake used Chrome 158.0.8127.88 and produced 192 distinct images per view. Hardware, browser and encoder versions can affect rendered bytes. [Materials, source links and reuse terms](../../tools/landscapes/assets/ASSETS.md)

Each `.frames` file starts with a little-endian 32-bit frame count followed by `count + 1` little-endian 32-bit offsets relative to the first byte of the JPEG payload. Each successive pair defines one independent image, including the terminal offset. Playback needs only the current frame and one decoded successor. JPEGs are decoded on demand. The desktop does not need Chrome, Three.js or an online service to show these scenes.

## Add a bundled view

Add its offline JPEG poster, optional frame pack and an entry in [manifest.json](manifest.json). Each view has a stable lowercase ID, a stable family shared by the perspectives of one landscape, a human-readable name and attribution. Keep IDs stable across visual revisions so saved choices reopen. A scenic-only scene needs one entry. A second angle gets another entry in the same family. The order in the manifest is the chooser order. The generated catalog supplies lookup and the candidates for shuffle.

Include both `frames` and `fps` for motion. Omit both for poster-only artwork. The build accepts 1â€“30 frames per second. Carbon ships the current seven views at 24 with `"playback":"loop"`. Forward loops continue through the last-to-first seam in sequence. Omitting the field preserves the original back-and-forth playback for existing assets. The build requires regular local assets and rejects duplicate IDs, missing attribution, unsafe filenames and unsupported manifest versions. Initial display and motion-decoding recovery use the checked-in poster. Pausing retains the current camera frame. Each frame pack is decoded off the UI thread, one image at a time. The previous atlas image is retired after painting. Hidden and reduced-motion behavior is controlled by the Home view's playback state.

Run `cargo test --locked -p carbon-desktop-gpui --lib`. The build also compiles a test-only catalog with an additional scenic-only perspective. The test locates it through the same catalog scan, checks its family, opens its packaged poster and motion stream, selects it with the production shuffle algorithm, and confirms it stays outside the shipped chooser. Ordinary builds use the assets directly and skip the artist's bake. Forks can provide their own compatible poster and frame pack without changing the native renderer or launcher UI.


The optional `bake` object identifies an explicit authoring source and camera, for example `{"source":"farmland","camera":"overhead"}`. Stable view IDs can be renamed independently of this binding. The authoring source registry maps names to actual local geometry modules and camera vectors. The build rejects unknown sources and cameras. Imported finished assets omit `bake` and are never overwritten by the baker. To author a new model, add its local geometry module and named camera definitions to [the source registry](../../tools/landscapes/sources.json). To reuse a finished pack and poster, only add the manifest record and the assets.
