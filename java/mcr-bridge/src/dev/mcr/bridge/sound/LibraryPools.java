package dev.mcr.bridge.sound;

import com.mojang.blaze3d.audio.Library;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.invoke.VarHandle;

/** Installs vanilla's private {@code CountingChannelPool}s, which source code cannot name. */
public final class LibraryPools {
    public static final int STATIC_CHANNELS = 247;
    public static final int STREAMING_CHANNELS = 8;

    private static final MethodHandle NEW_POOL;
    private static final VarHandle STATIC;
    private static final VarHandle STREAMING;

    static {
        try {
            ClassLoader loader = Library.class.getClassLoader();
            Class<?> pool = Class.forName("com.mojang.blaze3d.audio.Library$ChannelPool", false, loader);
            Class<?> counting = Class.forName("com.mojang.blaze3d.audio.Library$CountingChannelPool", false, loader);
            MethodHandles.Lookup lookup = MethodHandles.privateLookupIn(Library.class, MethodHandles.lookup());
            NEW_POOL = MethodHandles.privateLookupIn(counting, MethodHandles.lookup())
                    .findConstructor(counting, MethodType.methodType(void.class, int.class))
                    .asType(MethodType.methodType(Object.class, int.class));
            STATIC = lookup.findVarHandle(Library.class, "staticChannels", pool);
            STREAMING = lookup.findVarHandle(Library.class, "streamingChannels", pool);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("vanilla Library layout changed", e);
        }
    }

    public static void install(Library library) {
        try {
            STATIC.set(library, (Object) NEW_POOL.invokeExact(STATIC_CHANNELS));
            STREAMING.set(library, (Object) NEW_POOL.invokeExact(STREAMING_CHANNELS));
        } catch (Throwable t) {
            throw new IllegalStateException("cannot install the bridge channel pools", t);
        }
    }

    private LibraryPools() {}
}
