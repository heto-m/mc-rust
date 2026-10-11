package dev.mcr.bridge.net;

import dev.mcr.bridge.BridgeAbi;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Pose;

/** Rust owns the local player: its latest state overwrites Java's {@code LocalPlayer} after every Java tick. */
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

    static long appliedTick = -1;
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
        present = true;
    }

    public static void reset() {
        present = false;
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
