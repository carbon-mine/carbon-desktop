package dev.carbonmine.scenic.mixin;

import com.mojang.serialization.Lifecycle;
import dev.carbonmine.scenic.CaptureController;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.gui.screen.world.CreateWorldScreen;
import net.minecraft.server.integrated.IntegratedServerLoader;
import net.minecraft.world.level.storage.LevelStorage;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(IntegratedServerLoader.class)
public class IntegratedServerLoaderMixin {
    @Inject(method = "showBackupPromptScreen", at = @At("HEAD"), cancellable = true)
    private void carbon_skipBackupPrompt(LevelStorage.Session session, boolean customized, Runnable callback, Runnable onCancel, CallbackInfo ci) {
        if (CaptureController.isAutomated()) {
            callback.run();
            ci.cancel();
        }
    }

    @Inject(method = "tryLoad", at = @At("HEAD"), cancellable = true)
    private static void carbon_skipExperimentalWarning(MinecraftClient client, CreateWorldScreen parent, Lifecycle lifecycle, Runnable successCallback, boolean bypassWarnings, CallbackInfo ci) {
        if (CaptureController.isAutomated()) {
            successCallback.run();
            ci.cancel();
        }
    }
}
