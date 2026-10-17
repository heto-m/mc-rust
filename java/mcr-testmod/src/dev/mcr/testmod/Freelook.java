package dev.mcr.testmod;

/** The test freelook camera's offset from the player's rotation, turned by the mouse. */
public final class Freelook {
    public static float yaw;
    public static float pitch;

    private Freelook() {}

    public static void turn(double xo, double yo) {
        yaw += (float) xo * 0.15F;
        pitch += (float) yo * 0.15F;
    }
}
