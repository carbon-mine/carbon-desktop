package dev.carbonmine.scenic.mixin;

import net.minecraft.client.gui.screen.world.BackupPromptScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(BackupPromptScreen.class)
public interface BackupPromptScreenAccessor {
    @Accessor("callback")
    BackupPromptScreen.Callback carbon_getCallback();
}
