package dev.carbonmine.scenic.mixin;

import dev.carbonmine.scenic.CaptureController;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Pseudo;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyVariable;

@Pseudo
@Mixin(targets = "net.irisshaders.iris.uniforms.SystemTimeUniforms$Timer", remap = false)
public class IrisTimerMixin {
    @ModifyVariable(method = "beginFrame", at = @At("HEAD"), argsOnly = true, remap = false)
    private long carbon_syntheticIrisFrameTime(long frameStartTimeNs) {
        return CaptureController.getSyntheticIrisNanoTime(frameStartTimeNs);
    }
}
