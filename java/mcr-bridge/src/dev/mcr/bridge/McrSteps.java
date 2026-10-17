package dev.mcr.bridge;

import dev.mcr.bridge.mixin.EntityStepInvoker;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;

/**
 * Detects mods that replace the player's footsteps (Presence Footsteps): once a second Java asks vanilla for
 * a step sound of its shadow player; if that never reaches {@code playStepSound}, a mod took the footsteps
 * over and mcr stops playing its own.
 */
public final class McrSteps {
    private static final int PERIOD_TICKS = 20;

    private static int ticks;
    private static boolean probing;
    private static boolean reached;
    private static boolean replaced;

    private McrSteps() {}

    /** {@code LocalPlayer.tick} TAIL, after the shadow placed the player. */
    public static void tick(LocalPlayer player) {
        if (++ticks < PERIOD_TICKS) {
            return;
        }
        ticks = 0;
        BlockPos on = player.getOnPos();
        BlockState state = player.level().getBlockState(on);
        if (state.isAir()) {
            state = Blocks.STONE.defaultBlockState();
        }
        probing = true;
        reached = false;
        try {
            ((EntityStepInvoker) player).mcr$walkingStepSound(on, state);
        } finally {
            probing = false;
        }
        replaced = !reached;
    }

    /** {@code Player.playStepSound} HEAD: during a probe the step is only noted, never played. */
    public static boolean probeReached() {
        if (probing) {
            reached = true;
        }
        return probing;
    }

    static boolean replaced() {
        return replaced;
    }

    public static void reset() {
        replaced = false;
        ticks = 0;
    }
}
