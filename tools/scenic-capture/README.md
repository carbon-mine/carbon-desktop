# Carbon Desktop Scenic Capture Pipeline

Automated Fabric `1.21.1` recorder and loop encoder used to capture 1440p (`3840x2160` supersampled down to `2560x1440` @ `60fps`) seamless circular-camera and aerial backdrops from modded PolyMC worlds for Carbon Desktop.

## Components

- `src/main/java/dev/carbonmine/scenic/`: Fabric client mod (`carbon-scenic-recorder`) that boots directly into configured disposable singleplayer world copies, enforces capture resolution (`3840x2160` SSAA), sets weather/time locks, switches Iris shaderpacks per shot, moves the camera smoothly along a closed 3D elliptical/circular or lateral path (`mode`, `orbitRadius`, `radiusH`, `radiusV`), locks Iris + Minecraft tick clocks to synthetic 60 fps (`IrisTimerMixin`, `RenderTickCounterMixin`), and streams raw `rgba` frames to `ffmpeg`. The default writes lossless `ffv1` `.mkv` under `raw/`; `"directEncode": true` writes a bounded high-quality H.264 intermediate under `raw-avc/` instead, with an explicit free-space guard and `ffprobe` readback of frame count, timing, range and BT.709 color tags.
- `presets.json`: Finalized 6 builds across 10 viewpoints (`1,440` loop frames + `60` overlap frames @ `60fps` = `24.0s`, using serene ambient `8.5°–17°` orbital arcs, `radiusH = 2.6–3.2` lateral sweeps, and `phaseOffsetDeg = 90.0`) configuration for `tuscany_hills`, `cherry_city`, `kingdom`, `mistral_bay`, `nordic_village`, and `hobbit_hill` (all `CC BY-SA 4.0` from `OvercastCommunity/CommunityMaps`).
- `encode_scenery.py`: Accepts either `raw/<id>.mkv` or `raw-avc/<id>.mp4`, crossfades tail `[frames, frames + overlap)` into `[0, overlap)` with a cosine schedule for zero jump at the loop seam, downscales `4K -> 1440p` via `Lanczos`, encodes `H.264 High` (`-bf 3 -g 130 -crf 24`–`27` with boosted seam QP zones; `libx264` with `aq-mode=3` default or `h264_nvenc` VBR CQ with spatial/temporal AQ; `yuv420p`, `bt709` TV range) `2560x1440` `.mp4` + `2560x1440` `.jpg` posters, and emits stride-6 `960x540` `240`-frame `@ 10fps` `.frames` + matching `.jpg` bundled fallback assets.

## World safety

The recorder refuses to open a world unless the save folder contains an empty marker file named `.carbon-capture-disposable`. Copy the source world into `<instance>/.minecraft/saves/<name>`, add that marker, and delete the copy after capture. `save-off` reduces writes while the world is running, but the integrated server still performs a shutdown save; do not point the recorder at a source save.

## Disk-bounded capture

The lossless FFV1 path can need several gigabytes per scene. When free space is tight, set:

```json
"directEncode": true,
"directEncodeCrf": 12,
"minimumFreeMiB": 1536
```

The recorder then writes a hidden temporary MP4 in `raw-avc/` with a high-quality H.264 intermediate, verifies the frame count, frame rate, pixel format, range and BT.709 tags with `ffprobe`, and only then replaces `raw-avc/<id>.mp4`. A partial file is deleted on failure, so a previously verified capture is preserved. `encode_scenery.py` reads that intermediate, applies the same cosine seam blend, and writes the final package. This adds one near-lossless encode generation but removes the multi-gigabyte raw intermediate; keep `minimumFreeMiB` above the expected intermediate plus final outputs and never capture multiple scenes concurrently.

## Reproduce

1. Build `carbon-scenic-recorder`:
   ```powershell
   gradle --no-daemon build
   ```
   Copy `build/libs/carbon-scenic-recorder-0.1.0.jar` into `<PolyMC instance>/.minecraft/mods/` alongside Sodium, Iris, Fabric API, Falling Leaves, Subtle Effects, Particular, Wakes, Better Leaves, and shaders (`photon_v1.4.zip`, `ComplementaryUnbound_r5.10 + EuphoriaPatches_1.11.2`, `Bliss_v2.1.2_(Chocapic13_Shaders_edit).zip`).
2. Copy `presets.json` to `<PolyMC instance>/.minecraft/config/carbon_capture.json` (set `"mode": "scout"` first for fast single-frame composition previews, or `"mode": "capture"` for full `1,500`-frame capture). Set `"directEncode": true` for the bounded H.264 intermediate path, or leave it false for lossless FFV1. `"minimumFreeMiB"` is checked before the encoder starts. Then launch via PolyMC:
   ```powershell
   polymc.exe --launch carbon_scenery_1.21.1 -a <account>
   ```
3. Encode, verify loop seams, and stage the 1440p @ 60fps runtime scenery package:
   ```powershell
   python tools/scenic-capture/encode_scenery.py --config tools/scenic-capture/presets.json --avc-dir <avc1440_dir> --bundled-dir design-system/landscapes --encoder x264 --crf 24 --bundled-quality 62
   node tools/landscapes/check-loop.mjs <avc1440_dir>/<id>.mp4
   python scripts/stage-scenery-package.py --source <avc1440_dir> --output <exe_output_dir> --fps 60 --frames 1440
   ```
