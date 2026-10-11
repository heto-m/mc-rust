package dev.mcr.bridge.mixin;

import net.minecraft.network.Connection;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(Connection.class)
public interface ConnectionAccessor {
    @Invoker("flushQueue")
    void mcr$flushQueue();
}
