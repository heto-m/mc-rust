package dev.mcr.bridge.net;

import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.SwingAnimation;

/**
 * Rust owns the local player: its latest state overwrites Java's {@code LocalPlayer} after every Java tick, and its
 * predicted actions (swings, drops, mining) replay on Java's player as vanilla's own prediction would.
 */
public final class PlayerShadow {
    private static final ValueLayout.OfLong LONG = ValueLayout.JAVA_LONG_UNALIGNED;
    private static final ValueLayout.OfDouble DOUBLE = ValueLayout.JAVA_DOUBLE_UNALIGNED;
    private static final ValueLayout.OfFloat FLOAT = ValueLayout.JAVA_FLOAT_UNALIGNED;
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;

    private static boolean present;
    private static long tick;
    private static final double[] pos = new double[3];
    private static final double[] oldPos = new double[3];
    private static final double[] velocity = new double[3];
    private static float yaw;
    private static float pitch;
    private static int flags;
    private static int pose;
    private static int slot;
    private static int input;

    static long appliedTick = -1;
    /** Swings started for Rust's actions (probe answers). */
    static int swings;
    static final double[] appliedPos = new double[3];

    private PlayerShadow() {}

    static void receive(MemorySegment p) {
        if (p.byteSize() < BridgeAbi.PLAYER_SIZE) {
            return;
        }
        tick = p.get(LONG, BridgeAbi.PLAYER_TICK);
        for (int i = 0; i < 3; i++) {
            pos[i] = p.get(DOUBLE, BridgeAbi.PLAYER_POS + 8L * i);
            oldPos[i] = p.get(DOUBLE, BridgeAbi.PLAYER_OLD_POS + 8L * i);
            velocity[i] = p.get(DOUBLE, BridgeAbi.PLAYER_VELOCITY + 8L * i);
        }
        yaw = p.get(FLOAT, BridgeAbi.PLAYER_YAW);
        pitch = p.get(FLOAT, BridgeAbi.PLAYER_PITCH);
        flags = p.get(INT, BridgeAbi.PLAYER_FLAGS);
        pose = p.get(INT, BridgeAbi.PLAYER_POSE);
        slot = p.get(INT, BridgeAbi.PLAYER_SLOT);
        input = p.get(INT, BridgeAbi.PLAYER_INPUT);
        present = true;
    }

    /** The movement keys Rust's player moved with on its last tick. */
    public static Input input() {
        if (!present) {
            return Input.EMPTY;
        }
        return new Input(
                (input & BridgeAbi.INPUT_FORWARD) != 0,
                (input & BridgeAbi.INPUT_BACKWARD) != 0,
                (input & BridgeAbi.INPUT_LEFT) != 0,
                (input & BridgeAbi.INPUT_RIGHT) != 0,
                (input & BridgeAbi.INPUT_JUMP) != 0,
                (input & BridgeAbi.INPUT_SHIFT) != 0,
                (input & BridgeAbi.INPUT_SPRINT) != 0);
    }

    /** {@code NET_ACTION}, applied as it is drained so it lands between the packets around it. */
    static void action(Minecraft minecraft, MemorySegment p) {
        LocalPlayer player = minecraft.player;
        if (p.byteSize() < BridgeAbi.ACTION_SIZE || player == null || minecraft.level == null) {
            return;
        }
        int arg = p.get(INT, BridgeAbi.ACTION_ARG);
        switch (p.get(INT, BridgeAbi.ACTION_KIND)) {
            case BridgeAbi.ACTION_SWING -> {
                InteractionHand hand =
                        (arg & BridgeAbi.SWING_OFF_HAND) != 0 ? InteractionHand.OFF_HAND : InteractionHand.MAIN_HAND;
                ItemStack held = player.getItemInHand(hand);
                SwingAnimation animation =
                        (arg & BridgeAbi.SWING_ATTACK) != 0 ? held.getAttackAnimation() : held.getInteractAnimation();
                if (player.swing(hand, animation, false)) {
                    swings++;
                }
            }
            case BridgeAbi.ACTION_DROP -> {
                if (!player.getInventory().removeFromSelected(arg == 1).isEmpty()
                        && player.swing(InteractionHand.MAIN_HAND, SwingAnimation.DEFAULT, false)) {
                    swings++;
                }
            }
            case BridgeAbi.ACTION_DESTROY -> {
                BlockPos pos = new BlockPos(
                        p.get(INT, BridgeAbi.ACTION_POS), p.get(INT, BridgeAbi.ACTION_POS + 4), p.get(INT, BridgeAbi.ACTION_POS + 8));
                float progress = p.get(FLOAT, BridgeAbi.ACTION_VALUE);
                if (Float.isFinite(progress) && minecraft.level.isLoaded(pos)) {
                    minecraft.getTutorial()
                            .onDestroyBlock(minecraft.level, pos, minecraft.level.getBlockState(pos), Math.clamp(progress, -1f, 1f));
                }
            }
            case BridgeAbi.ACTION_RESET_ATTACK -> player.resetAttackStrengthTicker();
            default -> {}
        }
    }

    public static void reset() {
        dev.mcr.bridge.McrSteps.reset();
        dev.mcr.bridge.McrTurn.reset();
        present = false;
        input = 0;
        appliedTick = -1;
    }

    /** {@code LocalPlayer.tick} TAIL. */
    public static void apply(LocalPlayer player) {
        if (!present || !McrNet.bridged()) {
            return;
        }
        if (!player.isPassenger()) {
            player.setPos(pos[0], pos[1], pos[2]);
            player.xo = oldPos[0];
            player.yo = oldPos[1];
            player.zo = oldPos[2];
            player.xOld = oldPos[0];
            player.yOld = oldPos[1];
            player.zOld = oldPos[2];
            player.setDeltaMovement(velocity[0], velocity[1], velocity[2]);
            player.setOnGround((flags & BridgeAbi.PLAYER_ON_GROUND) != 0);
        }
        player.yRotO = player.getYRot();
        player.xRotO = player.getXRot();
        player.setYRot(yaw);
        player.setXRot(pitch);
        player.setYHeadRot(yaw);
        player.setShiftKeyDown((flags & BridgeAbi.PLAYER_SHIFT) != 0);
        player.setSprinting((flags & BridgeAbi.PLAYER_SPRINT) != 0);
        player.setSwimming((flags & BridgeAbi.PLAYER_SWIM) != 0);
        player.setPose(Pose.BY_ID.apply(pose));
        if (slot >= 0 && slot < 9) {
            player.getInventory().setSelectedSlot(slot);
        }
        appliedTick = tick;
        appliedPos[0] = player.getX();
        appliedPos[1] = player.getY();
        appliedPos[2] = player.getZ();
    }
}
