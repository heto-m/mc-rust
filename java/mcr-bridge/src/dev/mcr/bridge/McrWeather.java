package dev.mcr.bridge;

/**
 * Detects mods that replace vanilla's rain and snow: the level reached its weather step but vanilla's
 * extraction never ran its body (cancelled at its head, redirected or overwritten).
 */
public final class McrWeather {
    private static boolean expected;
    private static boolean ran;

    private McrWeather() {}

    public static void step() {
        expected = true;
        ran = false;
    }

    public static void vanillaRan() {
        ran = true;
    }

    static boolean takeReplaced() {
        boolean replaced = expected && !ran;
        expected = false;
        ran = false;
        return replaced;
    }
}
