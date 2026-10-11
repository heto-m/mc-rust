package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.commands.GpuQueryPool;
import java.util.Arrays;
import java.util.OptionalLong;

/** Timestamp queries are out of scope: every value stays empty. */
record McrQueryPool(int size) implements GpuQueryPool {
    @Override
    public OptionalLong getValue(int index) {
        return OptionalLong.empty();
    }

    @Override
    public OptionalLong[] getValues(int index, int count) {
        OptionalLong[] out = new OptionalLong[count];
        Arrays.fill(out, OptionalLong.empty());
        return out;
    }

    @Override
    public void close() {}
}
