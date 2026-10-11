package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.DeviceList;
import com.mojang.blaze3d.audio.DeviceTracker;
import com.mojang.blaze3d.audio.Library;
import com.mojang.blaze3d.audio.PollingDeviceTracker;
import dev.mcr.bridge.Bridge;
import java.util.OptionalLong;
import org.jspecify.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Library.class)
abstract class LibraryMixin {
    @Inject(method = "tryOpenDevice", at = @At("HEAD"))
    private static void mcr$countOpen(@Nullable String name, CallbackInfoReturnable<OptionalLong> cir) {
        Bridge.info("openal_open_device", name);
    }

    @Inject(method = "createDeviceTracker", at = @At("HEAD"), cancellable = true)
    private static void mcr$noDeviceTracking(CallbackInfoReturnable<DeviceTracker> cir) {
        cir.setReturnValue(new PollingDeviceTracker(DeviceList.EMPTY));
    }
}
