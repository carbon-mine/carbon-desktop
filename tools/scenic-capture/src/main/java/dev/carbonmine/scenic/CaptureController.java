package dev.carbonmine.scenic;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonObject;
import com.mojang.blaze3d.platform.GlStateManager;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.gl.Framebuffer;
import net.minecraft.client.texture.NativeImage;
import net.minecraft.client.util.ScreenshotRecorder;
import net.minecraft.server.integrated.IntegratedServer;
import net.minecraft.util.math.Box;
import net.minecraft.world.GameMode;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL30;
import org.lwjgl.system.MemoryUtil;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.lang.reflect.Method;
import java.nio.ByteBuffer;
import java.nio.file.FileStore;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

public final class CaptureController {
    private static final Logger LOGGER = LoggerFactory.getLogger("carbon-scenic-recorder");
    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();

    private enum Phase {
        IDLE,
        WAITING_FOR_WORLD,
        INITIAL_LOAD_SETTLE,
        APPLY_VIEWPOINT,
        VIEWPOINT_SETTLE,
        SCOUT_SNAPSHOT,
        CAPTURE_FRAMES,
        DISCONNECTING,
        DONE
    }

    private static CaptureConfig config;
    private static boolean automated = false;
    private static Phase phase = Phase.IDLE;
    private static int worldIndex = 0;
    private static int viewIndex = 0;
    private static int settleTicksRemaining = 0;
    private static int frameIndex = 0;
    private static int totalFramesToCapture = 0;
    private static String currentLoadedShader = "";

    private static boolean cameraOverrideActive = false;
    private static double camX, camY, camZ;
    private static float camYaw, camPitch;
    private static double camFov = 52.0;

    private static boolean forcingResolution = false;
    private static boolean deterministicTimeActive = false;
    private static long syntheticTickBaseMs = 0L;
    private static long syntheticTickTimeMs = 0L;
    private static long syntheticIrisBaseNs = 0L;
    private static long syntheticIrisTimeNs = 0L;
    private static int scoutShotStep = 0;
    private static int settleFramesRendered = 0;
    private static int stableTerrainFrames = 0;
    private static int lastCompletedChunks = -1;
    private static int lastLoadedChunks = -1;
    private static final int SETTLE_SWEEP_FRAMES = 140;
    private static final int SETTLE_HOLD_FRAMES = 120;
    private static final int SETTLE_MAX_FRAMES = 720;

    private static Process ffmpegProc;
    private static OutputStream ffmpegStdin;
    private static Path currentOutput;
    private static boolean currentDirectEncode;
    private static ByteBuffer readbackBuffer;
    private static byte[] transferBytes;
    private static final List<String> ffmpegErrors = new ArrayList<>();

    private CaptureController() {}

    public static void init(Path gameDir) {
        String prop = System.getProperty("carbon.capture.config");
        Path cfgPath = (prop != null && !prop.isBlank())
                ? Path.of(prop)
                : gameDir.resolve("config").resolve("carbon_capture.json");
        if (!Files.isRegularFile(cfgPath)) {
            LOGGER.info("No capture config at {}; passive mode.", cfgPath);
            return;
        }
        try {
            config = GSON.fromJson(Files.readString(cfgPath), CaptureConfig.class);
            if (config != null && config.worlds != null && !config.worlds.isEmpty()) {
                automated = true;
                phase = Phase.WAITING_FOR_WORLD;
                Files.createDirectories(Path.of(config.outputDir));
                LOGGER.info("Loaded {} capture worlds (mode={}, {}x{} @ {}fps)",
                        config.worlds.size(), config.mode, config.width, config.height, config.fps);
            }
        } catch (Exception e) {
            LOGGER.error("Failed to read capture config from {}", cfgPath, e);
        }
    }

    public static boolean isAutomated() {
        return automated;
    }

    public static boolean isCameraOverrideActive() {
        return automated && cameraOverrideActive;
    }

    public static boolean isForcingResolution() {
        return automated && forcingResolution;
    }

    public static boolean isDeterministicTimeActive() {
        return automated && deterministicTimeActive;
    }

    public static int targetWidth() {
        return config != null ? config.width : 2560;
    }

    public static int targetHeight() {
        return config != null ? config.height : 1440;
    }

    public static double getCamX() { return camX; }
    public static double getCamY() { return camY; }
    public static double getCamZ() { return camZ; }
    public static float getCamYaw() { return camYaw; }
    public static float getCamPitch() { return camPitch; }
    public static double getCamFov() { return camFov; }

    public static long getSyntheticTickTimeMs(long realMillis) {
        if (!deterministicTimeActive) {
            syntheticTickBaseMs = realMillis;
            syntheticTickTimeMs = realMillis;
            return realMillis;
        }
        return syntheticTickTimeMs;
    }

    public static long getSyntheticIrisNanoTime(long realNanos) {
        return deterministicTimeActive ? syntheticIrisTimeNs : realNanos;
    }

    public static void logScreen(Object screen) {
        LOGGER.info("Screen opened (phase={}): {}", phase, screen != null ? screen.getClass().getName() : "null");
    }

    public static void onTitleScreenReady(MinecraftClient client) {
        if (!automated) return;
        if (phase == Phase.DONE || worldIndex >= config.worlds.size()) {
            LOGGER.info("All scenic capture jobs complete; shutting down.");
            phase = Phase.DONE;
            client.scheduleStop();
            return;
        }
        if (phase == Phase.WAITING_FOR_WORLD || phase == Phase.DISCONNECTING) {
            CaptureConfig.WorldJob job = config.worlds.get(worldIndex);
            viewIndex = 0;
            LOGGER.info("Launching world {}/{}: {}", worldIndex + 1, config.worlds.size(), job.world);
            phase = Phase.INITIAL_LOAD_SETTLE;
            settleTicksRemaining = 40;
            client.createIntegratedServerLoader().start(job.world, () -> {
                LOGGER.error("World load cancelled for {}", job.world);
                phase = Phase.DONE;
                client.scheduleStop();
            });
        }
    }

    public static void onClientTick(MinecraftClient client) {
        if (!automated) return;
        if (client.world == null || client.player == null || client.getServer() == null) {
            if ((phase == Phase.WAITING_FOR_WORLD || phase == Phase.DISCONNECTING)
                    && client.getOverlay() == null
                    && client.currentScreen != null) {
                String cls = client.currentScreen.getClass().getName();
                if (cls.endsWith("TitleScreen")) {
                    onTitleScreenReady(client);
                } else if (cls.endsWith("AccessibilityOnboardingScreen")) {
                    client.setScreen(new net.minecraft.client.gui.screen.TitleScreen());
                }
            }
            return;
        }

        if (phase == Phase.INITIAL_LOAD_SETTLE) {
            if (settleTicksRemaining > 0) {
                settleTicksRemaining--;
                return;
            }
            prepareGraphicsAndWorld(client, config.worlds.get(worldIndex));
            phase = Phase.APPLY_VIEWPOINT;
            return;
        }

        if (phase == Phase.APPLY_VIEWPOINT) {
            CaptureConfig.WorldJob job = config.worlds.get(worldIndex);
            if (viewIndex >= job.viewpoints.size()) {
                finishWorld(client);
                return;
            }
            CaptureConfig.Viewpoint vp = job.viewpoints.get(viewIndex);
            String desiredShader = vp.shader != null ? vp.shader : job.shader;
            applyShaderIfNeeded(desiredShader);
            long tod = vp.timeOfDay != null ? vp.timeOfDay : job.timeOfDay;
            String weather = vp.weather != null ? vp.weather : job.weather;
            runWorldCommands(client.getServer(), tod, weather, vp.eye);
            client.player.setVelocity(0, 0, 0);
            client.player.setPosition(vp.eye[0], vp.eye[1], vp.eye[2]);

            camFov = vp.fov;
            updateCameraPose(vp, 0);
            cameraOverrideActive = true;
            settleFramesRendered = 0;
            stableTerrainFrames = 0;
            lastCompletedChunks = -1;
            lastLoadedChunks = -1;
            scoutShotStep = 0;
            phase = Phase.VIEWPOINT_SETTLE;
            LOGGER.info("Settling viewpoint {} (sweep={} + hold>={} frames, motion={})...",
                    vp.id, SETTLE_SWEEP_FRAMES, SETTLE_HOLD_FRAMES, vp.motion);
            return;
        }

        if (phase == Phase.VIEWPOINT_SETTLE) {
            if (client.currentScreen != null) {
                client.setScreen(null);
            }
            syncPlayerToCamera(client);
        }
    }

    private static boolean isAllTerrainComplete(MinecraftClient client) {
        if (client.worldRenderer == null || !client.worldRenderer.isTerrainRenderComplete()) {
            return false;
        }
        try {
            Class<?> cls = Class.forName("net.caffeinemc.mods.sodium.client.render.SodiumWorldRenderer");
            Object inst = cls.getMethod("instanceNullable").invoke(null);
            if (inst != null) {
                Object complete = cls.getMethod("isTerrainRenderComplete").invoke(inst);
                if (complete instanceof Boolean b && !b) {
                    return false;
                }
            }
        } catch (Throwable ignored) {
            // Sodium absent or API changed; rely on vanilla completion + chunk stability count.
        }
        return true;
    }

    public static void onBeforeFrameRender(MinecraftClient client) {
        if (!automated) return;
        if (phase == Phase.VIEWPOINT_SETTLE && client.world != null) {
            if (client.currentScreen != null) {
                client.setScreen(null);
            }
            CaptureConfig.Viewpoint vp = config.worlds.get(worldIndex).viewpoints.get(viewIndex);
            if (settleFramesRendered < SETTLE_SWEEP_FRAMES) {
                int probeFrame = (settleFramesRendered * Math.max(1, vp.frames)) / SETTLE_SWEEP_FRAMES;
                updateCameraPose(vp, probeFrame % Math.max(1, vp.frames));
            } else {
                updateCameraPose(vp, 0);
            }
            syncPlayerToCamera(client);
        } else if (phase == Phase.SCOUT_SNAPSHOT && client.world != null) {
            CaptureConfig.Viewpoint vp = config.worlds.get(worldIndex).viewpoints.get(viewIndex);
            int[] sampleOffsets = "orbit".equalsIgnoreCase(vp.motion)
                    ? new int[]{0, vp.frames / 4, vp.frames / 2, (vp.frames * 3) / 4}
                    : new int[]{0, vp.frames / 4, (vp.frames * 3) / 4};
            int targetFrame = sampleOffsets[Math.min(scoutShotStep, sampleOffsets.length - 1)];
            updateCameraPose(vp, targetFrame);
            syncPlayerToCamera(client);
        } else if (phase == Phase.CAPTURE_FRAMES && client.world != null) {
            if (client.currentScreen != null) {
                client.setScreen(null);
            }
            CaptureConfig.Viewpoint vp = config.worlds.get(worldIndex).viewpoints.get(viewIndex);
            updateCameraPose(vp, frameIndex);
            syncPlayerToCamera(client);
            syntheticTickTimeMs = syntheticTickBaseMs + (long) Math.floor((frameIndex + 1) * 1000.0 / config.fps);
            syntheticIrisTimeNs = syntheticIrisBaseNs + (long) Math.floor((frameIndex + 1) * 1_000_000_000.0 / config.fps);
        }
    }

    public static void onAfterWorldRender(MinecraftClient client) {
        if (!automated || client.world == null) return;
        try {
            renderCapturedFrame(client);
        } catch (RuntimeException e) {
            LOGGER.error("Scenic capture failed; cleaning up and stopping", e);
            abortFfmpeg("capture");
            deterministicTimeActive = false;
            phase = Phase.DONE;
            client.scheduleStop();
        }
    }

    private static void renderCapturedFrame(MinecraftClient client) {
        if (phase == Phase.VIEWPOINT_SETTLE) {
            CaptureConfig.Viewpoint vp = config.worlds.get(worldIndex).viewpoints.get(viewIndex);
            settleFramesRendered++;
            boolean settled = false;
            if (settleFramesRendered >= SETTLE_SWEEP_FRAMES) {
                int completed = client.worldRenderer.getCompletedChunkCount();
                int loaded = client.world.getChunkManager().getLoadedChunkCount();
                boolean complete = isAllTerrainComplete(client);
                if (complete && completed > 0 && completed == lastCompletedChunks && loaded == lastLoadedChunks) {
                    stableTerrainFrames++;
                } else {
                    stableTerrainFrames = 0;
                    lastCompletedChunks = completed;
                    lastLoadedChunks = loaded;
                }
                if ((settleFramesRendered >= SETTLE_SWEEP_FRAMES + SETTLE_HOLD_FRAMES && stableTerrainFrames >= 60)
                        || settleFramesRendered >= SETTLE_MAX_FRAMES) {
                    LOGGER.info("Terrain settled for {} after {} frames (completed={}, loaded={}, stable={})",
                            vp.id, settleFramesRendered, completed, loaded, stableTerrainFrames);
                    settled = true;
                }
            }
            if (settled) {
                updateCameraPose(vp, 0);
                syncPlayerToCamera(client);
                if ("scout".equalsIgnoreCase(config.mode)) {
                    phase = Phase.SCOUT_SNAPSHOT;
                } else {
                    logPathObstructions(client, vp);
                    startFfmpegForViewpoint(vp);
                    frameIndex = 0;
                    totalFramesToCapture = vp.frames + vp.overlapFrames;
                    syntheticTickBaseMs = syntheticTickTimeMs;
                    syntheticIrisBaseNs = System.nanoTime();
                    syntheticIrisTimeNs = syntheticIrisBaseNs;
                    deterministicTimeActive = true;
                    phase = Phase.CAPTURE_FRAMES;
                    LOGGER.info("Capturing {} frames ({} loop + {} overlap) at {}x{} -> {}x{} @ {}fps for {}",
                            totalFramesToCapture, vp.frames, vp.overlapFrames,
                            config.width, config.height, outWidth(), outHeight(), config.fps, vp.id);
                }
            }
        } else if (phase == Phase.SCOUT_SNAPSHOT) {
            CaptureConfig.Viewpoint vp = config.worlds.get(worldIndex).viewpoints.get(viewIndex);
            if (scoutShotStep == 0) {
                logPathObstructions(client, vp);
            }
            String[] suffixes = "orbit".equalsIgnoreCase(vp.motion)
                    ? new String[]{"", "_q1", "_q2", "_q3"}
                    : new String[]{"", "_r", "_l"};
            if (scoutShotStep < suffixes.length) {
                GL11.glFinish();
                saveScoutPng(client.getFramebuffer(), vp.id + suffixes[scoutShotStep]);
                scoutShotStep++;
            }
            if (scoutShotStep >= suffixes.length) {
                viewIndex++;
                phase = Phase.APPLY_VIEWPOINT;
            }
        } else if (phase == Phase.CAPTURE_FRAMES) {
            CaptureConfig.Viewpoint vp = config.worlds.get(worldIndex).viewpoints.get(viewIndex);
            writeFrameToFfmpeg(client.getFramebuffer());
            frameIndex++;
            if (frameIndex % Math.max(1, config.fps) == 0) {
                LOGGER.info("  {} progress: {}/{} frames", vp.id, frameIndex, totalFramesToCapture);
            }
            if (frameIndex >= totalFramesToCapture) {
                stopFfmpeg(vp.id);
                deterministicTimeActive = false;
                viewIndex++;
                phase = Phase.APPLY_VIEWPOINT;
            }
        }
    }

    private static void prepareGraphicsAndWorld(MinecraftClient client, CaptureConfig.WorldJob job) {
        client.options.hudHidden = true;
        client.options.pauseOnLostFocus = false;
        client.options.getBobView().setValue(false);
        client.options.getFovEffectScale().setValue(0.0);
        if (!forcingResolution) {
            forcingResolution = true;
            client.onResolutionChanged();
            var fb = client.getFramebuffer();
            LOGGER.info("Forced resolution -> framebuffer is {}x{}", fb.textureWidth, fb.textureHeight);
        }
        applyShaderIfNeeded(job.shader);
        runWorldCommands(client.getServer(), job.timeOfDay, job.weather, null);
        if (job.commands != null && !job.commands.isEmpty() && client.getServer() != null) {
            var server = client.getServer();
            server.execute(() -> {
                var mgr = server.getCommandManager();
                var src = server.getCommandSource().withSilent();
                for (String cmd : job.commands) {
                    if (cmd != null && !cmd.isBlank()) {
                        mgr.executeWithPrefix(src, cmd);
                    }
                }
            });
        }
    }

    private static void finishWorld(MinecraftClient client) {
        cameraOverrideActive = false;
        deterministicTimeActive = false;
        var server = client.getServer();
        if (server != null) {
            server.execute(() -> {
                for (var world : server.getWorlds()) {
                    world.savingDisabled = true;
                }
            });
        }
        worldIndex++;
        phase = (worldIndex >= config.worlds.size()) ? Phase.DONE : Phase.DISCONNECTING;
        LOGGER.info("Disconnecting from current world...");
        if (client.world != null) {
            client.world.disconnect();
        }
        client.disconnect();
        if (phase == Phase.DONE) {
            LOGGER.info("All scenic capture jobs complete; shutting down.");
            client.scheduleStop();
        } else {
            client.setScreen(new net.minecraft.client.gui.screen.TitleScreen());
        }
    }

    private static void runWorldCommands(IntegratedServer server, long timeOfDay, String weather, double[] tp) {
        if (server == null) return;
        server.execute(() -> {
            for (var world : server.getWorlds()) {
                world.savingDisabled = true;
            }
            var mgr = server.getCommandManager();
            var src = server.getCommandSource().withSilent();
            mgr.executeWithPrefix(src, "save-off");
            mgr.executeWithPrefix(src, "gamerule doDaylightCycle false");
            mgr.executeWithPrefix(src, "gamerule doWeatherCycle false");
            mgr.executeWithPrefix(src, "gamerule doMobSpawning false");
            mgr.executeWithPrefix(src, "gamerule sendCommandFeedback false");
            mgr.executeWithPrefix(src, "time set " + timeOfDay);
            mgr.executeWithPrefix(src, "weather " + (weather != null ? weather : "clear") + " 1000000");
            for (var player : server.getPlayerManager().getPlayerList()) {
                player.changeGameMode(GameMode.SPECTATOR);
                if (tp != null) {
                    player.requestTeleport(tp[0], tp[1], tp[2]);
                }
            }
        });
    }

    private static void applyShaderIfNeeded(String shaderPack) {
        if (shaderPack == null || shaderPack.isBlank() || shaderPack.equals(currentLoadedShader)) {
            return;
        }
        try {
            Class<?> irisCls = Class.forName("net.irisshaders.iris.Iris");
            Method getCfg = irisCls.getMethod("getIrisConfig");
            Object cfg = getCfg.invoke(null);
            if (cfg != null) {
                cfg.getClass().getMethod("setShaderPackName", String.class).invoke(cfg, shaderPack);
                cfg.getClass().getMethod("setShadersEnabled", boolean.class).invoke(cfg, true);
                cfg.getClass().getMethod("save").invoke(cfg);
                irisCls.getMethod("reload").invoke(null);
                currentLoadedShader = shaderPack;
                LOGGER.info("Switched Iris shaderpack to {}", shaderPack);
            }
        } catch (Exception e) {
            LOGGER.warn("Could not switch Iris shaderpack to {}: {}", shaderPack, e.toString());
        }
    }

    private static int outWidth() {
        return (config != null && config.outputWidth > 0) ? config.outputWidth : targetWidth();
    }

    private static int outHeight() {
        return (config != null && config.outputHeight > 0) ? config.outputHeight : targetHeight();
    }

    private static void syncPlayerToCamera(MinecraftClient client) {
        if (client.player == null) return;
        client.player.setVelocity(0, 0, 0);
        client.player.setPosition(camX, camY, camZ);
        client.player.prevX = camX;
        client.player.prevY = camY;
        client.player.prevZ = camZ;
        client.player.setYaw(camYaw);
        client.player.setPitch(camPitch);
    }

    private static void logPathObstructions(MinecraftClient client, CaptureConfig.Viewpoint vp) {
        if (client.world == null) return;
        double savedX = camX, savedY = camY, savedZ = camZ;
        float savedYaw = camYaw, savedPitch = camPitch;
        int hits = 0;
        for (int i = 0; i < 32; i++) {
            int probe = (i * Math.max(1, vp.frames)) / 32;
            updateCameraPose(vp, probe);
            Box clearance = Box.of(new net.minecraft.util.math.Vec3d(camX, camY, camZ), 1.8, 1.8, 1.8);
            if (!client.world.isSpaceEmpty(clearance)) {
                hits++;
                LOGGER.warn("Obstruction near {} at phase {}/32 pos=({},{},{})",
                        vp.id, i, Math.round(camX), Math.round(camY), Math.round(camZ));
            }
        }
        if (hits == 0) {
            LOGGER.info("Clearance check PASSED for {} (32/32 orbit points clear)", vp.id);
        }
        camX = savedX;
        camY = savedY;
        camZ = savedZ;
        camYaw = savedYaw;
        camPitch = savedPitch;
    }

    private static void updateCameraPose(CaptureConfig.Viewpoint vp, int frame) {
        double u = (double) frame / Math.max(1, vp.frames);
        double phase = 2.0 * Math.PI * u + Math.toRadians(vp.phaseOffsetDeg);

        if ("orbit".equalsIgnoreCase(vp.motion)) {
            double dx0 = vp.eye[0] - vp.target[0];
            double dz0 = vp.eye[2] - vp.target[2];
            double radius = (vp.orbitRadius > 0.0) ? vp.orbitRadius : Math.max(4.0, Math.hypot(dx0, dz0));
            double startTheta = Math.atan2(dz0, dx0);
            double dir = (vp.orbitDir < 0.0) ? -1.0 : 1.0;
            double theta = Math.abs(vp.sweepDeg - 360.0) < 1e-3
                    ? startTheta + dir * 2.0 * Math.PI * u
                    : startTheta + dir * Math.toRadians(vp.sweepDeg * 0.5) * Math.sin(phase);
            double r = radius + vp.radiusH * Math.sin(phase);
            camX = vp.target[0] + r * Math.cos(theta);
            camZ = vp.target[2] + r * Math.sin(theta);
            camY = vp.eye[1] + vp.radiusV * Math.sin(phase);
        } else {
            double fx = vp.target[0] - vp.eye[0];
            double fz = vp.target[2] - vp.eye[2];
            double hLen = Math.hypot(fx, fz);
            double fhx = (hLen > 1e-4) ? (fx / hLen) : 0.0;
            double fhz = (hLen > 1e-4) ? (fz / hLen) : 1.0;
            // Horizontal right vector = f_h x (0, 1, 0) = (-fhz, 0, fhx)
            double rx = -fhz;
            double rz = fhx;
            double lat = vp.radiusH * Math.sin(phase);
            double dep = vp.radiusDepth * (Math.cos(phase) - 1.0);
            camX = vp.eye[0] + lat * rx + dep * fhx;
            camY = vp.eye[1] + vp.radiusV * Math.sin(phase);
            camZ = vp.eye[2] + lat * rz + dep * fhz;
        }

        double dx = vp.target[0] - camX;
        double dy = vp.target[1] - camY;
        double dz = vp.target[2] - camZ;
        double horiz = Math.hypot(dx, dz);
        camYaw = (float) (Math.toDegrees(Math.atan2(dz, dx)) - 90.0);
        camPitch = (float) (-Math.toDegrees(Math.atan2(dy, horiz)));
    }

    private static void saveScoutPng(Framebuffer fb, String id) {
        try {
            Path scoutDir = Path.of(config.outputDir, "scout");
            Files.createDirectories(scoutDir);
            Path out = scoutDir.resolve(id + ".png");
            try (NativeImage img = ScreenshotRecorder.takeScreenshot(fb)) {
                img.writeTo(out);
            }
            LOGGER.info("Saved scout screenshot: {} ({}x{})", out, fb.textureWidth, fb.textureHeight);
        } catch (Exception e) {
            LOGGER.error("Failed to save scout screenshot for {}", id, e);
        }
    }

    private static void startFfmpegForViewpoint(CaptureConfig.Viewpoint vp) {
        try {
            Path outputDir = Path.of(config.outputDir, config.directEncode ? "raw-avc" : "raw");
            Files.createDirectories(outputDir);
            if (config.directEncode) requireFreeSpace(outputDir, config.minimumFreeMiB, vp.id);
            Path output = outputDir.resolve(vp.id + (config.directEncode ? ".mp4" : ".mkv"));
            int w = config.width;
            int h = config.height;
            int neededBytes = w * h * 4;
            if (readbackBuffer == null || readbackBuffer.capacity() != neededBytes) {
                if (readbackBuffer != null) MemoryUtil.memFree(readbackBuffer);
                readbackBuffer = MemoryUtil.memAlloc(neededBytes);
                transferBytes = new byte[neededBytes];
            }
            ffmpegErrors.clear();
            int ow = outWidth();
            int oh = outHeight();
            String vf = (ow != w || oh != h)
                    ? ("vflip,scale=" + ow + ":" + oh + ":flags=lanczos+accurate_rnd+full_chroma_int")
                    : "vflip";
            if (config.directEncode) {
                vf = vf + ",scale=in_range=full:out_range=limited:out_color_matrix=bt709,format=yuv420p,"
                        + "setparams=range=limited:color_primaries=bt709:color_trc=iec61966-2-1:colorspace=bt709";
            }
            List<String> cmd = new ArrayList<>(List.of(
                    config.ffmpegPath,
                    "-hide_banner", "-loglevel", "error", "-y",
                    "-f", "rawvideo",
                    "-pix_fmt", "rgba",
                    "-s", w + "x" + h,
                    "-r", String.valueOf(config.fps),
                    "-i", "pipe:0",
                    "-vf", vf
            ));
            if (config.directEncode) {
                cmd.addAll(List.of(
                        "-c:v", "libx264", "-profile:v", "high", "-preset", "medium",
                        "-crf", String.valueOf(Math.max(0, config.directEncodeCrf)),
                        "-bf", "0", "-g", String.valueOf(Math.max(1, config.fps * 2)), "-sc_threshold", "0",
                        "-color_range", "tv", "-colorspace", "bt709",
                        "-color_primaries", "bt709", "-color_trc", "iec61966-2-1",
                        "-x264-params", "colorprim=bt709:transfer=iec61966-2-1:colormatrix=bt709",
                        "-movflags", "+write_colr",
                        output.toString()
                ));
            } else {
                cmd.addAll(List.of(
                        "-c:v", "ffv1",
                        "-level", "3",
                        "-threads", "6",
                        output.toString()
                ));
            }
            ProcessBuilder pb = new ProcessBuilder(cmd);
            pb.redirectErrorStream(false);
            ffmpegProc = pb.start();
            ffmpegStdin = ffmpegProc.getOutputStream();
            currentOutput = output;
            currentDirectEncode = config.directEncode;
            Thread errThread = new Thread(() -> {
                try (var br = new BufferedReader(new InputStreamReader(ffmpegProc.getErrorStream()))) {
                    String line;
                    while ((line = br.readLine()) != null) {
                        ffmpegErrors.add(line);
                        LOGGER.error("[ffmpeg {}] {}", vp.id, line);
                    }
                } catch (IOException ignored) {}
            }, "carbon-ffmpeg-stderr");
            errThread.setDaemon(true);
            errThread.start();
        } catch (Exception e) {
            abortFfmpeg(vp.id);
            throw new RuntimeException("Failed to start FFmpeg for " + vp.id, e);
        }
    }

    private static void requireFreeSpace(Path directory, int minimumMiB, String id) throws IOException {
        if (minimumMiB <= 0) return;
        FileStore store = Files.getFileStore(directory);
        long required = minimumMiB * 1024L * 1024L;
        long available = store.getUsableSpace();
        if (available < required) {
            throw new IOException("Only " + available + " bytes free at " + directory
                    + "; direct capture for " + id + " requires at least " + required + " bytes");
        }
        LOGGER.info("Disk headroom for {}: {} MiB available, {} MiB required",
                id, available / 1024L / 1024L, minimumMiB);
    }

    private static void writeFrameToFfmpeg(Framebuffer fb) {
        int w = fb.textureWidth;
        int h = fb.textureHeight;
        if (w != config.width || h != config.height) {
            throw new IllegalStateException("Framebuffer size mismatch: " + w + "x" + h + " != " + config.width + "x" + config.height);
        }
        GL11.glFinish();
        GlStateManager._glBindFramebuffer(GL30.GL_READ_FRAMEBUFFER, fb.fbo);
        GL11.glPixelStorei(GL11.GL_PACK_ALIGNMENT, 1);
        readbackBuffer.clear();
        GL11.glReadPixels(0, 0, w, h, GL11.GL_RGBA, GL11.GL_UNSIGNED_BYTE, readbackBuffer);
        readbackBuffer.rewind();
        readbackBuffer.get(transferBytes);
        try {
            ffmpegStdin.write(transferBytes);
        } catch (IOException e) {
            throw new RuntimeException("Failed writing raw frame to FFmpeg", e);
        }
    }

    private static void stopFfmpeg(String id) {
        Path output = currentOutput;
        try {
            if (ffmpegStdin != null) {
                ffmpegStdin.flush();
                ffmpegStdin.close();
                ffmpegStdin = null;
            }
            if (ffmpegProc != null) {
                int code = ffmpegProc.waitFor();
                ffmpegProc = null;
                if (code != 0) {
                    throw new RuntimeException("FFmpeg exited with code " + code + " for " + id + ": " + String.join("; ", ffmpegErrors));
                }
                if (currentDirectEncode) {
                    verifyDirectOutput(output, id, totalFramesToCapture);
                    LOGGER.info("Finished verified direct H.264 capture for {} at {}", id, output);
                } else {
                    LOGGER.info("Finished lossless capture for {}", id);
                }
            }
        } catch (Exception e) {
            abortFfmpeg(id);
            throw new RuntimeException("Failed finishing FFmpeg for " + id, e);
        } finally {
            currentOutput = null;
            currentDirectEncode = false;
        }
    }

    private static void abortFfmpeg(String id) {
        Path output = currentOutput;
        if (ffmpegStdin != null) {
            try {
                ffmpegStdin.close();
            } catch (IOException ignored) {
            }
            ffmpegStdin = null;
        }
        if (ffmpegProc != null) {
            ffmpegProc.destroy();
            try {
                if (!ffmpegProc.waitFor(5, java.util.concurrent.TimeUnit.SECONDS)) {
                    ffmpegProc.destroyForcibly();
                }
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                ffmpegProc.destroyForcibly();
            }
            ffmpegProc = null;
        }
        if (output != null) {
            try {
                Files.deleteIfExists(output);
            } catch (IOException cleanupError) {
                LOGGER.error("Could not remove partial capture {}", output, cleanupError);
            }
        }
        currentOutput = null;
        currentDirectEncode = false;
        LOGGER.warn("Aborted FFmpeg capture for {} and removed any partial output", id);
    }

    private static void verifyDirectOutput(Path output, String id, int expectedFrames) throws Exception {
        Path ffmpeg = Path.of(config.ffmpegPath);
        Path sibling = ffmpeg.getParent() == null
                ? null
                : ffmpeg.getParent().resolve(System.getProperty("os.name", "").toLowerCase().contains("win")
                        ? "ffprobe.exe" : "ffprobe");
        String probe = (sibling != null && Files.isRegularFile(sibling)) ? sibling.toString() : "ffprobe";
        Process process = new ProcessBuilder(
                probe,
                "-v", "error",
                "-count_frames",
                "-select_streams", "v:0",
                "-show_entries",
                "stream=codec_name,pix_fmt,width,height,nb_frames,nb_read_frames,r_frame_rate,color_range,color_space,color_primaries,color_transfer",
                "-of", "json",
                output.toString())
                .redirectErrorStream(true)
                .start();
        String text = new String(process.getInputStream().readAllBytes(), java.nio.charset.StandardCharsets.UTF_8);
        int code = process.waitFor();
        if (code != 0) {
            throw new IllegalStateException("ffprobe failed for " + id + ": " + text);
        }
        JsonObject root = GSON.fromJson(text, JsonObject.class);
        if (root == null || !root.has("streams") || root.getAsJsonArray("streams").size() != 1) {
            throw new IllegalStateException("Expected one video stream for " + id + ": " + text);
        }
        JsonObject stream = root.getAsJsonArray("streams").get(0).getAsJsonObject();
        requireField(stream, "codec_name", "h264", id);
        requireField(stream, "pix_fmt", "yuv420p", id);
        requireField(stream, "width", String.valueOf(outWidth()), id);
        requireField(stream, "height", String.valueOf(outHeight()), id);
        requireFrameCount(stream, expectedFrames, id);
        requireField(stream, "r_frame_rate", config.fps + "/1", id);
        requireField(stream, "color_range", "tv", id);
        requireField(stream, "color_space", "bt709", id);
        requireField(stream, "color_primaries", "bt709", id);
        requireField(stream, "color_transfer", "iec61966-2-1", id);
    }

    private static void requireFrameCount(JsonObject stream, int expected, String id) {
        String count = stream.has("nb_read_frames") && !stream.get("nb_read_frames").isJsonNull()
                ? stream.get("nb_read_frames").getAsString()
                : null;
        if (count == null || "N/A".equals(count)) {
            count = stream.has("nb_frames") && !stream.get("nb_frames").isJsonNull()
                    ? stream.get("nb_frames").getAsString()
                    : null;
        }
        if (count == null || "N/A".equals(count) || !String.valueOf(expected).equals(count)) {
            throw new IllegalStateException("Direct capture " + id + " has frame count " + count
                    + ", expected " + expected);
        }
    }

    private static void requireField(JsonObject stream, String field, String expected, String id) {
        String actual = stream.has(field) && !stream.get(field).isJsonNull()
                ? stream.get(field).getAsString()
                : "<missing>";
        if (!expected.equals(actual)) {
            throw new IllegalStateException("Direct capture " + id + " has " + field + "=" + actual
                    + ", expected " + expected);
        }
    }
}
