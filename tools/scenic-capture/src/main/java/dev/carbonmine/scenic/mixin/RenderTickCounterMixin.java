package dev.carbonmine.scenic.mixin;

import dev.carbonmine.scenic.CaptureController;
import net.minecraft.client.render.RenderTickCounter;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyVariable;

@Mixin(RenderTickCounter.Dynamic.class)
public class RenderTickCounterMixin {
    @ModifyVariable(method = "beginRenderTick(JZ)I", at = @At("HEAD"), argsOnly = true)
    private long carbon_syntheticTickMillis(long timeMillis) {
        return CaptureController.getSyntheticTickTimeMs(timeMillis);
    }
}
