package dev.carbonmine.scenic.mixin;

import dev.carbonmine.scenic.CaptureController;
import net.minecraft.client.util.Window;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Window.class)
public class WindowMixin {
    @Shadow
    private int framebufferWidth;

    @Shadow
    private int framebufferHeight;

    @Inject(method = "getFramebufferWidth", at = @At("HEAD"), cancellable = true)
    private void carbon_forceWidth(CallbackInfoReturnable<Integer> cir) {
        if (CaptureController.isForcingResolution()) {
            cir.setReturnValue(CaptureController.targetWidth());
        }
    }

    @Inject(method = "getFramebufferHeight", at = @At("HEAD"), cancellable = true)
    private void carbon_forceHeight(CallbackInfoReturnable<Integer> cir) {
        if (CaptureController.isForcingResolution()) {
            cir.setReturnValue(CaptureController.targetHeight());
        }
    }

    @Inject(method = "updateFramebufferSize", at = @At("TAIL"))
    private void carbon_lockUpdateSize(CallbackInfo ci) {
        if (CaptureController.isForcingResolution()) {
            this.framebufferWidth = CaptureController.targetWidth();
            this.framebufferHeight = CaptureController.targetHeight();
        }
    }

    @Inject(method = "onFramebufferSizeChanged", at = @At("HEAD"), cancellable = true)
    private void carbon_ignoreResize(long window, int width, int height, CallbackInfo ci) {
        if (CaptureController.isForcingResolution()) {
            this.framebufferWidth = CaptureController.targetWidth();
            this.framebufferHeight = CaptureController.targetHeight();
            ci.cancel();
        }
    }
}
