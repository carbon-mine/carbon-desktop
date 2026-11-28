package dev.carbonmine.scenic.mixin;

import dev.carbonmine.scenic.CaptureController;
import net.minecraft.client.MinecraftClient;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(MinecraftClient.class)
public class MinecraftClientMixin {
    @Inject(method = "render", at = @At("HEAD"))
    private void carbon_beforeRender(boolean tick, CallbackInfo ci) {
        CaptureController.onBeforeFrameRender((MinecraftClient) (Object) this);
    }

    @Inject(
        method = "render",
        at = @At(
            value = "INVOKE",
            target = "Lnet/minecraft/client/render/GameRenderer;render(Lnet/minecraft/client/render/RenderTickCounter;Z)V",
            shift = At.Shift.AFTER
        )
    )
    private void carbon_afterGameRenderer(boolean tick, CallbackInfo ci) {
        CaptureController.onAfterWorldRender((MinecraftClient) (Object) this);
    }
}
