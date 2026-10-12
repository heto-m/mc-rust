package dev.mcr.bridge;

/**
 * Which vanilla particles Java keeps. mcr draws the vanilla particles of the code paths it mirrors (packets,
 * entity, block entity and level ticks); inside those Java drops {@code minecraft:*} types, elsewhere (mod code)
 * it keeps them. Render thread only.
 */
public final class McrParticles {
    private static int mirrored;
    private static int mod;
    private static long dropped;

    private McrParticles() {}

    public static void enter() {
        mirrored++;
    }

    public static void exit() {
        if (mirrored > 0) {
            mirrored--;
        }
    }

    /** Mod code running inside a mirrored path, such as a mod's custom payload handler. */
    public static void enterMod() {
        mod++;
    }

    public static void exitMod() {
        if (mod > 0) {
            mod--;
        }
    }

    /** A handler that threw skipped its exit; every frame starts outside all scopes. */
    static void reset() {
        mirrored = 0;
        mod = 0;
    }

    /** A vanilla particle created now is one mcr draws itself. */
    public static boolean mcrDraws() {
        return mirrored > 0 && mod == 0;
    }

    public static void countDropped() {
        dropped++;
    }

    public static long dropped() {
        return dropped;
    }
}
