package dev.carbonmine.scenic;

import dev.carbonmine.scenic.mixin.BackupPromptScreenAccessor;
import dev.carbonmine.scenic.mixin.ConfirmScreenAccessor;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.MinecraftClient;
import net.minecraft.client.gui.screen.AccessibilityOnboardingScreen;
import net.minecraft.client.gui.screen.ConfirmScreen;
import net.minecraft.client.gui.screen.NoticeScreen;
import net.minecraft.client.gui.screen.TitleScreen;
import net.minecraft.client.gui.screen.world.BackupPromptScreen;

public class CarbonScenicMod implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        CaptureController.init(FabricLoader.getInstance().getGameDir());

        ScreenEvents.AFTER_INIT.register((client, screen, scaledWidth, scaledHeight) -> {
            if (!CaptureController.isAutomated()) return;
            CaptureController.logScreen(screen);
            if (screen instanceof AccessibilityOnboardingScreen) {
                client.execute(() -> client.setScreen(new TitleScreen()));
            } else if (screen instanceof TitleScreen) {
                client.execute(() -> CaptureController.onTitleScreenReady(client));
            } else if (screen instanceof BackupPromptScreen backup) {
                client.execute(() -> ((BackupPromptScreenAccessor) backup).carbon_getCallback().proceed(false, false));
            } else if (screen instanceof ConfirmScreen confirm) {
                client.execute(() -> ((ConfirmScreenAccessor) confirm).carbon_getCallback().accept(true));
            } else if (screen instanceof NoticeScreen) {
                client.execute(() -> client.setScreen(null));
            }
        });

        ClientTickEvents.END_CLIENT_TICK.register(CaptureController::onClientTick);
    }
}
