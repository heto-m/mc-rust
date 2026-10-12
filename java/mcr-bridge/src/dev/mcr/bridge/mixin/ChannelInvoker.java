package dev.mcr.bridge.mixin;

import com.mojang.blaze3d.audio.Channel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(Channel.class)
public interface ChannelInvoker {
    @Invoker("<init>")
    static Channel mcr$new(int source) {
        throw new AssertionError();
    }
}
