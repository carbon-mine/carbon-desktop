#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Convert circular Minecraft captures into Carbon Desktop 1440p MP4s and
960x540 bundled fallback frame packs.

The capture mod records (frames + overlap) frames along a periodic circular path,
so input[i] and input[i + frames] share the exact same camera position and
orientation. Cross-fading [frames, frames + overlap) into [0, overlap) with a
cosine schedule keeps static geometry sharp while smoothing water, foliage, and
cloud phase back to frame 0 with zero jump across the loop seam.

The recorder can write either the original lossless FFV1 Matroska files under
``raw/`` or bounded high-quality H.264 intermediates under ``raw-avc/``. The
encoder accepts either input and always emits the same final H.264 output.
"""
from __future__ import annotations

import argparse
import io
import json
import math
import os
import shutil
import struct
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image


def loop_quality_zones(count: int, fps: int, crf: int = 23) -> str:
    ramp = min(max(10, round(fps * 0.35)), math.ceil(count / 8))
    hold = min(3, ramp // 4)
    seam_qp = min(11, max(8, crf - 12))
    mid_qp = max(14, crf - 6)
    tail_start = max(ramp, count - ramp)
    return "/".join(
        [
            f"0,{hold},q={seam_qp}",
            f"{hold + 1},{ramp - 1},q={mid_qp}",
            f"{tail_start},{count - hold - 2},q={mid_qp}",
            f"{count - hold - 1},{count - 1},q={seam_qp}",
        ]
    )


def read_exact(stream, size: int) -> bytes:
    buf = bytearray()
    while len(buf) < size:
        chunk = stream.read(size - len(buf))
        if not chunk:
            raise RuntimeError(f"Unexpected EOF from ffmpeg (got {len(buf)}/{size} bytes)")
        buf.extend(chunk)
    return bytes(buf)


def supports_nvenc(ffmpeg: str) -> bool:
    """Probe whether h264_nvenc is usable on the active GPU."""
    try:
        probe = subprocess.run(
            [
                ffmpeg, "-hide_banner", "-loglevel", "error", "-y",
                "-f", "lavfi", "-i", "color=c=black:s=256x256:r=60:d=0.05",
                "-c:v", "h264_nvenc", "-preset", "p4", "-f", "null", "-",
            ],
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=10,
        )
        return probe.returncode == 0
    except OSError:
        return False


def build_encode_command(
    ffmpeg: str,
    width: int,
    height: int,
    fps: int,
    frames: int,
    crf: int,
    vf: str,
    mp4_tmp: Path,
    encoder: str,
) -> list[str]:
    base = [
        ffmpeg, "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{width}x{height}", "-r", str(fps),
        "-i", "pipe:0",
        "-map", "0:v:0", "-an", "-sn", "-dn",
        "-vf", vf,
    ]
    color = [
        "-color_range", "tv", "-colorspace", "bt709",
        "-color_primaries", "bt709", "-color_trc", "iec61966-2-1",
    ]
    if encoder == "nvenc":
        # High Profile + B-frames VBR keeps 1440p60 backdrops compact and WMF-compatible
        gop = min(frames, max(70, fps * 2 + 10))
        return base + [
            "-c:v", "h264_nvenc", "-profile:v", "high", "-preset", "p7", "-tune", "hq",
            "-rc", "vbr", "-cq", str(crf), "-b:v", "0", "-maxrate", "24M", "-bufsize", "48M",
            "-bf", "3", "-b_ref_mode", "middle", "-g", str(gop),
            "-spatial-aq", "1", "-temporal-aq", "1", "-aq-strength", "8", "-rc-lookahead", "32",
        ] + color + ["-movflags", "+faststart+write_colr", str(mp4_tmp)]
    gop = min(frames, max(70, fps * 2 + 10))
    return base + [
        "-c:v", "libx264", "-profile:v", "high", "-preset", "slow", "-crf", str(crf),
        "-bf", "3", "-g", str(gop), "-sc_threshold", "0",
    ] + color + [
        "-x264-params", f"colorprim=bt709:transfer=iec61966-2-1:colormatrix=bt709:aq-mode=3:zones={loop_quality_zones(frames, fps, crf)}",
        "-movflags", "+faststart+write_colr", str(mp4_tmp),
    ]


def process_clip(
    capture_input: Path,
    view_id: str,
    width: int,
    height: int,
    fps: int,
    frames: int,
    overlap: int,
    avc_dir: Path,
    bundled_dir: Path,
    ffmpeg: str,
    ffprobe: str,
    crf: int,
    bundled_quality: int,
    encoder: str = "x264",
) -> None:
    frame_bytes = width * height * 3
    avc_dir.mkdir(parents=True, exist_ok=True)
    bundled_dir.mkdir(parents=True, exist_ok=True)

    # 1. Extract overlap tail [frames, frames + overlap)
    tail: list[np.ndarray] = []
    if overlap > 0:
        tail_proc = subprocess.Popen(
            [
                ffmpeg, "-hide_banner", "-loglevel", "error",
                "-i", str(capture_input),
                "-vf", f"trim=start_frame={frames}:end_frame={frames + overlap},setpts=PTS-STARTPTS",
                "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
            ],
            stdout=subprocess.PIPE,
        )
        assert tail_proc.stdout is not None
        for _ in range(overlap):
            raw = read_exact(tail_proc.stdout, frame_bytes)
            tail.append(np.frombuffer(raw, dtype=np.uint8).reshape((height, width, 3)).copy())
        if tail_proc.wait() != 0:
            raise RuntimeError(f"FFmpeg tail extraction failed for {view_id}")

    # 2. Stream [0, frames), blend head with tail, pipe to H.264 + build 960x540 .frames
    mp4_tmp = avc_dir / f".{view_id}.tmp.mp4"
    mp4_out = avc_dir / f"{view_id}.mp4"
    jpg_out = avc_dir / f"{view_id}.jpg"

    vf = (
        "scale=in_range=full:out_range=limited:out_color_matrix=bt709,"
        "format=yuv420p,"
        "setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709"
    )
    enc_cmd = build_encode_command(
        ffmpeg=ffmpeg,
        width=width,
        height=height,
        fps=fps,
        frames=frames,
        crf=crf,
        vf=vf,
        mp4_tmp=mp4_tmp,
        encoder=encoder,
    )
    enc_proc = subprocess.Popen(enc_cmd, stdin=subprocess.PIPE)
    assert enc_proc.stdin is not None

    src_proc = subprocess.Popen(
        [
            ffmpeg, "-hide_banner", "-loglevel", "error",
            "-i", str(capture_input),
            "-frames:v", str(frames),
            "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
        ],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
    )
    assert src_proc.stdout is not None

    bundled_stride = max(1, math.ceil(frames / 240))
    while bundled_stride < frames and (frames % bundled_stride != 0 or fps % bundled_stride != 0):
        bundled_stride += 1
    if frames % bundled_stride != 0:
        bundled_stride = 1
    bundled_jpegs: list[bytes] = []
    for i in range(frames):
        raw = read_exact(src_proc.stdout, frame_bytes)
        cur = np.frombuffer(raw, dtype=np.uint8).reshape((height, width, 3))
        if i < overlap:
            alpha = 0.5 * (1.0 - math.cos(math.pi * i / overlap))
            blended = (
                (1.0 - alpha) * tail[i].astype(np.float32) + alpha * cur.astype(np.float32)
            ).round().clip(0, 255).astype(np.uint8)
        else:
            blended = cur

        enc_proc.stdin.write(blended.tobytes())

        img = Image.fromarray(blended, mode="RGB")
        if i == 0:
            img.save(jpg_out, format="JPEG", quality=94, optimize=True)

        if i % bundled_stride == 0:
            small = img.resize((960, 540), Image.Resampling.LANCZOS)
            buf = io.BytesIO()
            small.save(buf, format="JPEG", quality=bundled_quality, optimize=True)
            bundled_jpegs.append(buf.getvalue())

    enc_proc.stdin.close()
    if src_proc.wait() != 0:
        raise RuntimeError(f"FFmpeg source read failed for {view_id}")
    if enc_proc.wait() != 0:
        raise RuntimeError(f"FFmpeg encode failed for {view_id}")

    # 3. Verify H.264 stream metadata matches Carbon Desktop's expectations
    probe = subprocess.check_output(
        [ffprobe, "-v", "error", "-show_streams", "-of", "json", str(mp4_tmp)],
        text=True,
    )
    streams = json.loads(probe)["streams"]
    s = streams[0]
    assert len(streams) == 1 and s["codec_name"] == "h264" and s["pix_fmt"] == "yuv420p"
    assert s["width"] == width and s["height"] == height
    assert int(s["nb_frames"]) == frames and s["r_frame_rate"] == f"{fps}/1"
    assert s["color_range"] == "tv" and s["color_space"] == "bt709"
    assert s["color_primaries"] == "bt709" and s["color_transfer"] == "iec61966-2-1"

    shutil.move(str(mp4_tmp), str(mp4_out))

    # 4. Write 960x540 .frames pack and matching opening .jpg poster
    frames_path = bundled_dir / f"{view_id}.frames"
    poster_path = bundled_dir / f"{view_id}.jpg"
    header = [len(bundled_jpegs)]
    offset = 0
    for jb in bundled_jpegs:
        header.append(offset)
        offset += len(jb)
    header.append(offset)
    with frames_path.with_suffix(".frames.next").open("wb") as handle:
        handle.write(struct.pack(f"<{len(header)}I", *header))
        for jb in bundled_jpegs:
            handle.write(jb)
    shutil.move(str(frames_path.with_suffix(".frames.next")), str(frames_path))
    poster_path.write_bytes(bundled_jpegs[0])
    bundled_fps = max(1, fps // bundled_stride)
    print(f"{view_id:24} -> mp4={mp4_out.stat().st_size/1024/1024:5.1f} MB ({fps}fps) | frames={frames_path.stat().st_size/1024/1024:5.1f} MB ({len(bundled_jpegs)}f@{bundled_fps}fps)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--avc-dir", required=True, type=Path)
    parser.add_argument("--bundled-dir", required=True, type=Path)
    parser.add_argument("--crf", type=int, default=24)
    parser.add_argument("--bundled-quality", type=int, default=62)
    parser.add_argument("--encoder", choices=("auto", "nvenc", "x264"), default="auto")
    parser.add_argument("--only", nargs="*", default=None)
    parser.add_argument("--unlink-raw", action="store_true")
    args = parser.parse_args()

    cfg = json.loads(args.config.read_text(encoding="utf-8"))
    output_dir = Path(cfg["outputDir"])
    raw_dir = output_dir / "raw"
    direct_dir = output_dir / "raw-avc"
    out_w = int(cfg.get("outputWidth") or cfg["width"])
    out_h = int(cfg.get("outputHeight") or cfg["height"])
    ffmpeg = cfg.get("ffmpegPath", "ffmpeg")
    ffprobe = str(Path(ffmpeg).with_name("ffprobe.exe" if os.name == "nt" else "ffprobe"))
    if not Path(ffprobe).exists():
        ffprobe = "ffprobe"
    encoder = "nvenc" if (args.encoder == "nvenc" or (args.encoder == "auto" and supports_nvenc(ffmpeg))) else "x264"
    print(f"Using encoder={encoder}", flush=True)

    for world in cfg["worlds"]:
        for vp in world["viewpoints"]:
            vid = vp["id"]
            if args.only and vid not in args.only:
                continue
            capture_input = raw_dir / f"{vid}.mkv"
            if not capture_input.exists():
                capture_input = direct_dir / f"{vid}.mp4"
            if not capture_input.exists():
                raise FileNotFoundError(
                    f"Missing capture for {vid}: expected {raw_dir / (vid + '.mkv')} "
                    f"or {direct_dir / (vid + '.mp4')}"
                )
            process_clip(
                capture_input=capture_input,
                view_id=vid,
                width=out_w,
                height=out_h,
                fps=cfg["fps"],
                frames=vp["frames"],
                overlap=vp.get("overlapFrames", 0),
                avc_dir=args.avc_dir,
                bundled_dir=args.bundled_dir,
                ffmpeg=ffmpeg,
                ffprobe=ffprobe,
                crf=args.crf,
                bundled_quality=args.bundled_quality,
                encoder=encoder,
            )
            if args.unlink_raw:
                capture_input.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
