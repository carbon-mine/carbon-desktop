package dev.carbonmine.scenic;

import java.util.ArrayList;
import java.util.List;

public class CaptureConfig {
    public String mode = "scout"; // "scout" or "capture"
    public int width = 3840;
    public int height = 2160;
    public int outputWidth = 2560;
    public int outputHeight = 1440;
    public int fps = 60;
    public String ffmpegPath = "ffmpeg";
    public String outputDir = "scenic-renders";
    /** Write a bounded high-quality H.264 intermediate instead of multi-gigabyte FFV1. */
    public boolean directEncode = false;
    public int directEncodeCrf = 12;
    public int minimumFreeMiB = 1536;
    public List<WorldJob> worlds = new ArrayList<>();

    public static class WorldJob {
        public String world;
        public String shader = "photon_v1.4.zip";
        public long timeOfDay = 23200;
        public String weather = "clear";
        public double settleSeconds = 6.0;
        public List<String> commands = new ArrayList<>();
        public List<Viewpoint> viewpoints = new ArrayList<>();
    }

    public static class Viewpoint {
        public String id;
        public String motion = "orbit"; // "orbit" around target, or "lateral" sweep
        public double[] eye = new double[]{0, 80, 0};
        public double[] target = new double[]{0, 70, 50};
        public double orbitRadius = 0.0;
        public double orbitDir = 1.0;
        public double sweepDeg = 360.0;
        public double radiusH = 0.0;
        public double radiusDepth = 3.5;
        public double radiusV = 0.0;
        public double phaseOffsetDeg = 0.0;
        public double fov = 52.0;
        public int frames = 720;
        public int overlapFrames = 90;
        public String shader; // optional per-viewpoint override
        public Long timeOfDay; // optional per-viewpoint override
        public String weather; // optional per-viewpoint override
    }
}
