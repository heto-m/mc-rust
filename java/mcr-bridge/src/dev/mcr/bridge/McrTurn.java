package dev.mcr.bridge;

/**
 * Detects mods that take the mouse's turning for themselves (Freelook): Java's {@code turnPlayer} runs, and
 * when the player's {@code turn} never reaches vanilla's body, a mod consumed it and mcr stops turning its
 * player until a turn reaches vanilla again. Vanilla's own turn is cancelled: Rust turns the player.
 */
public final class McrTurn {
    private static boolean expecting;
    private static boolean reached;
    private static boolean captured;

    private McrTurn() {}

    /** Right before {@code turnPlayer} turns the local player. */
    public static void before() {
        expecting = true;
        reached = false;
    }

    /** Vanilla's {@code Entity.turn} body for the local player: noted, and skipped because Rust turns. */
    public static boolean vanillaReached() {
        if (!expecting) {
            return false;
        }
        reached = true;
        return true;
    }

    /** Right after {@code turnPlayer} turned the local player. */
    public static void after() {
        if (expecting) {
            captured = !reached;
        }
        expecting = false;
    }

    static boolean captured() {
        return captured;
    }

    public static void reset() {
        expecting = false;
        captured = false;
    }
}
