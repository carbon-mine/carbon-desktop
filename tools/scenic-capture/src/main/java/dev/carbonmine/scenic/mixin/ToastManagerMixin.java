package dev.carbonmine.scenic.mixin;

import dev.carbonmine.scenic.CaptureController;
import net.minecraft.client.toast.Toast;
import net.minecraft.client.toast.ToastManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ToastManager.class)
public class ToastManagerMixin {
    @Inject(method = "add", at = @At("HEAD"), cancellable = true)
    private void carbon_suppressToasts(Toast toast, CallbackInfo ci) {
        if (CaptureController.isAutomated()) {
            ci.cancel();
        }
    }
}
