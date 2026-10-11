package dev.mcr.bridge.gpu;

/** A Java GPU object backed by a generation-checked Rust handle. */
interface McrHandle {
    long handle();
}
