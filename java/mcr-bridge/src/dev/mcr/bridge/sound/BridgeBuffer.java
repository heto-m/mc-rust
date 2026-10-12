package dev.mcr.bridge.sound;

/** Mixed into {@code SoundBuffer}: its PCM's id in Rust, uploaded on first use; 0 when unavailable. */
public interface BridgeBuffer {
    int mcr$bridgeId();
}
