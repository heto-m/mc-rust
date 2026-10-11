package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.commands.GpuFence;

/** Completion of one submit; waiting on the open submit flushes it. */
final class McrFence implements GpuFence {
    private final McrCommandEncoder encoder;
    private final long submit;
    private boolean done;

    McrFence(McrCommandEncoder encoder, long submit) {
        this.encoder = encoder;
        this.submit = submit;
    }

    @Override
    public boolean awaitCompletion(long timeoutNs) {
        if (!done) {
            done = encoder.await(submit, timeoutNs);
        }
        return done;
    }

    @Override
    public void close() {}
}
