package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.invoke.VarHandle;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Arrays;
import java.util.OptionalInt;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.core.BlockPos;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3dc;

/**
 * Publishes LambDynamicLights' active light sources after each frame, so mcr's terrain shader lights the
 * ground the way the mod would. The mod has no read API; its internals are looked up once by reflection, and
 * any miss turns the channel off for the session.
 */
public final class McrDynamicLights {
    private static final String MOD_ID = "lambdynlights";
    private static final String PKG = "dev.lambdaurora.lambdynlights.";
    private static final double FALLOFF = 15.0 / 7.75;
    /** Sources looked at per frame; the nearest `LIGHT_MAX` of them are published. */
    private static final int MAX_SCAN = 4096;
    private static final ValueLayout.OfInt INT = ValueLayout.JAVA_INT_UNALIGNED;
    private static final ValueLayout.OfFloat FLOAT = ValueLayout.JAVA_FLOAT_UNALIGNED;
    private static final ValueLayout.OfDouble DOUBLE = ValueLayout.JAVA_DOUBLE_UNALIGNED;
    private static final VarHandle SEQ = ValueLayout.JAVA_INT.varHandle();

    private static final double[] start = new double[MAX_SCAN * 3];
    private static final float[] span = new float[MAX_SCAN * 3];
    private static final float[] level = new float[MAX_SCAN];
    private static final long[] order = new long[MAX_SCAN];

    private static boolean looked;
    private static Hooks hooks;
    private static int seq;
    private static int count;

    private McrDynamicLights() {}

    /** Called at the end of each lockstep frame, before the frame state says it is done. */
    static void publish(Minecraft minecraft) {
        if (!looked) {
            looked = true;
            if (Loaders.mods().stream().anyMatch(m -> MOD_ID.equals(m.id()))) {
                hooks = Hooks.find();
            }
        }
        if (hooks == null) {
            return;
        }
        int flags = BridgeAbi.LIGHTS_PROVIDER;
        count = 0;
        try {
            if (hooks.enabled()) {
                flags |= BridgeAbi.LIGHTS_ENABLED;
                if (minecraft.level != null) {
                    collect(minecraft);
                }
            }
        } catch (Throwable t) {
            Bridge.info("dynamic_lights", "disabled: " + t);
            hooks = null;
            flags = 0;
            count = 0;
        }
        write(Bridge.get().lights, flags);
    }

    private static void collect(Minecraft minecraft) throws Throwable {
        float partial = McrFrame.partialTick();
        for (Object source : hooks.sources()) {
            if (count == MAX_SCAN) {
                break;
            }
            if (hooks.entitySource.isInstance(source)) {
                int luminance = (int) hooks.luminance.invokeExact(source);
                if (luminance <= 0) {
                    continue;
                }
                double x = (double) hooks.x.invokeExact(source);
                double y = (double) hooks.y.invokeExact(source);
                double z = (double) hooks.z.invokeExact(source);
                if (source instanceof Entity e) {
                    y += Mth.lerp(partial, e.yo, e.getY()) - e.getY();
                    x = Mth.lerp(partial, e.xo, e.getX());
                    z = Mth.lerp(partial, e.zo, e.getZ());
                }
                add(x, y, z, 0, 0, 0, luminance);
            } else if (hooks.deferred.isInstance(source)) {
                behaviour(hooks.behavior.invoke(source));
            }
        }
    }

    private static void behaviour(Object b) throws Throwable {
        if (hooks.line.isInstance(b)) {
            Vector3dc a = (Vector3dc) hooks.lineStart.invoke(b);
            Vector3dc e = (Vector3dc) hooks.lineEnd.invoke(b);
            int luminance = (int) hooks.lineLuminance.invoke(b);
            add(a.x(), a.y(), a.z(), e.x() - a.x(), e.y() - a.y(), e.z() - a.z(), luminance);
        } else if (hooks.beacon.isInstance(b)) {
            Level level = (Level) hooks.beaconLevel.invoke(b);
            OptionalInt base = (OptionalInt) hooks.beaconY.invoke(b);
            column((int) hooks.beaconX.invoke(b), base.orElse(level.getMinY()), (int) hooks.beaconZ.invoke(b),
                    level.getMaxY(), (int) hooks.beaconLuminance.invoke(b));
        } else if (hooks.gateway != null && hooks.gateway.isInstance(b)) {
            BlockEntity gateway = (BlockEntity) hooks.gatewayEntity.invoke(b);
            Level level = gateway.getLevel();
            if (level != null) {
                BlockPos pos = gateway.getBlockPos();
                column(pos.getX(), level.getMinY(), pos.getZ(), level.getMaxY(), gateway.getBlockState().getLightEmission());
            }
        } else {
            int[] box = hooks.box(b);
            BlockPos center = new BlockPos((box[0] + box[3]) >> 1, (box[1] + box[4]) >> 1, (box[2] + box[5]) >> 1);
            double luminance = (double) hooks.lightAtPos.invoke(b, center, FALLOFF);
            add(center.getX() + 0.5, center.getY() + 0.5, center.getZ() + 0.5, 0, 0, 0, luminance);
        }
    }

    private static void column(int x, int bottom, int z, int top, double luminance) {
        add(x + 0.5, bottom + 0.5, z + 0.5, 0, Math.max(0, top - bottom), 0, luminance);
    }

    private static void add(double x, double y, double z, double dx, double dy, double dz, double luminance) {
        if (!(luminance > 0) || count == MAX_SCAN) {
            return;
        }
        int i = count++;
        start[i * 3] = x;
        start[i * 3 + 1] = y;
        start[i * 3 + 2] = z;
        span[i * 3] = (float) dx;
        span[i * 3 + 1] = (float) dy;
        span[i * 3 + 2] = (float) dz;
        level[i] = (float) Math.min(luminance, 15.0);
    }

    /** The nearest sources first when there are more than the block holds. */
    private static int sort(Minecraft minecraft) {
        for (int i = 0; i < count; i++) {
            order[i] = i;
        }
        if (count <= BridgeAbi.LIGHT_MAX) {
            return count;
        }
        Vec3 eye = minecraft.gameRenderer.mainCamera().position();
        for (int i = 0; i < count; i++) {
            double dx = start[i * 3] - eye.x;
            double dy = start[i * 3 + 1] - eye.y;
            double dz = start[i * 3 + 2] - eye.z;
            float d = (float) Math.min(dx * dx + dy * dy + dz * dz, Float.MAX_VALUE);
            order[i] = (long) Float.floatToRawIntBits(d) << 32 | i;
        }
        Arrays.sort(order, 0, count);
        return BridgeAbi.LIGHT_MAX;
    }

    private static void write(MemorySegment block, int flags) {
        int n = sort(Minecraft.getInstance());
        SEQ.setRelease(block, (long) BridgeAbi.LIGHT_SEQ, ++seq);
        VarHandle.storeStoreFence();
        block.set(INT, BridgeAbi.LIGHT_FLAGS, flags);
        block.set(INT, BridgeAbi.LIGHT_COUNT, n);
        for (int k = 0; k < n; k++) {
            int i = (int) order[k];
            long at = BridgeAbi.LIGHT_ENTRIES + (long) k * BridgeAbi.LIGHT_ENTRY_SIZE;
            for (int c = 0; c < 3; c++) {
                block.set(DOUBLE, at + BridgeAbi.LIGHT_START + c * 8L, start[i * 3 + c]);
                block.set(FLOAT, at + BridgeAbi.LIGHT_END + c * 4L, span[i * 3 + c]);
            }
            block.set(FLOAT, at + BridgeAbi.LIGHT_LEVEL, level[i]);
        }
        SEQ.setRelease(block, (long) BridgeAbi.LIGHT_SEQ, ++seq);
    }

    /** LambDynamicLights' internals, resolved once. */
    private record Hooks(
            Object instance,
            Field sourcesField,
            MethodHandle mode,
            MethodHandle modeEnabled,
            Class<?> entitySource,
            MethodHandle x,
            MethodHandle y,
            MethodHandle z,
            MethodHandle luminance,
            Class<?> deferred,
            MethodHandle behavior,
            Class<?> line,
            MethodHandle lineStart,
            MethodHandle lineEnd,
            MethodHandle lineLuminance,
            Class<?> beacon,
            MethodHandle beaconX,
            MethodHandle beaconY,
            MethodHandle beaconZ,
            MethodHandle beaconLuminance,
            MethodHandle beaconLevel,
            Class<?> gateway,
            MethodHandle gatewayEntity,
            MethodHandle boundingBox,
            MethodHandle[] boxCorners,
            MethodHandle lightAtPos) {

        static Hooks find() {
            try {
                ClassLoader loader = McrDynamicLights.class.getClassLoader();
                MethodHandles.Lookup lookup = MethodHandles.lookup();
                Class<?> mod = Class.forName(PKG + "LambDynLights", true, loader);
                Object instance = mod.getMethod("get").invoke(null);
                Field sources = mod.getDeclaredField("dynamicLightSources");
                sources.setAccessible(true);
                Object config = mod.getField("config").get(instance);
                Method getMode = config.getClass().getMethod("getDynamicLightsMode");
                MethodHandle mode = lookup.unreflect(getMode).bindTo(config);
                MethodHandle enabled = lookup.unreflect(getMode.getReturnType().getMethod("isEnabled"));

                Class<?> entity = Class.forName(PKG + "engine.source.EntityDynamicLightSource", false, loader);
                MethodType toDouble = MethodType.methodType(double.class, Object.class);
                Class<?> deferred = Class.forName(PKG + "engine.source.DeferredDynamicLightSource", false, loader);
                Class<?> behaviour = Class.forName(PKG + "api.behavior.DynamicLightBehavior", false, loader);
                Class<?> line = Class.forName(PKG + "api.behavior.LineLightBehavior", false, loader);
                Class<?> beacon = Class.forName(PKG + "api.behavior.BeaconLightBehavior", false, loader);
                Class<?> gateway = optional(PKG + "echo.TheEndGatewayBeamLightBehavior", loader);
                Method box = behaviour.getMethod("getBoundingBox");
                Class<?> boxType = box.getReturnType();
                MethodHandle[] corners = new MethodHandle[6];
                String[] names = {"startX", "startY", "startZ", "endX", "endY", "endZ"};
                for (int i = 0; i < 6; i++) {
                    corners[i] = lookup.unreflect(boxType.getMethod(names[i]));
                }
                Hooks hooks = new Hooks(
                        instance,
                        sources,
                        mode,
                        enabled,
                        entity,
                        lookup.unreflect(entity.getMethod("getDynamicLightX")).asType(toDouble),
                        lookup.unreflect(entity.getMethod("getDynamicLightY")).asType(toDouble),
                        lookup.unreflect(entity.getMethod("getDynamicLightZ")).asType(toDouble),
                        lookup.unreflect(entity.getMethod("getLuminance")).asType(MethodType.methodType(int.class, Object.class)),
                        deferred,
                        lookup.unreflect(deferred.getMethod("behavior")),
                        line,
                        lookup.unreflect(line.getMethod("getStartPoint")),
                        lookup.unreflect(line.getMethod("getEndPoint")),
                        lookup.unreflect(line.getMethod("getLuminance")),
                        beacon,
                        lookup.unreflect(beacon.getMethod("x")),
                        lookup.unreflect(beacon.getMethod("y")),
                        lookup.unreflect(beacon.getMethod("z")),
                        lookup.unreflect(beacon.getMethod("luminance")),
                        lookup.unreflect(beacon.getMethod("level")),
                        gateway,
                        gateway == null ? null : lookup.unreflect(gateway.getMethod("gateway")),
                        lookup.unreflect(box),
                        corners,
                        lookup.unreflect(behaviour.getMethod("lightAtPos", BlockPos.class, double.class)));
                hooks.enabled();
                hooks.sources();
                Bridge.info("dynamic_lights", "LambDynamicLights");
                return hooks;
            } catch (Throwable t) {
                Bridge.info("dynamic_lights", "LambDynamicLights is loaded but unreadable: " + t);
                return null;
            }
        }

        private static Class<?> optional(String name, ClassLoader loader) {
            try {
                return Class.forName(name, false, loader);
            } catch (ClassNotFoundException e) {
                return null;
            }
        }

        boolean enabled() throws Throwable {
            return (boolean) modeEnabled.invoke(mode.invoke());
        }

        Set<?> sources() throws IllegalAccessException {
            return (Set<?>) sourcesField.get(instance);
        }

        int[] box(Object behaviour) throws Throwable {
            Object b = boundingBox.invoke(behaviour);
            int[] out = new int[6];
            for (int i = 0; i < 6; i++) {
                out[i] = (int) boxCorners[i].invoke(b);
            }
            return out;
        }
    }
}
