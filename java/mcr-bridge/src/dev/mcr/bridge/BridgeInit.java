package dev.mcr.bridge;

import dev.mcr.boot.Native;
import java.util.concurrent.locks.LockSupport;
import net.minecraft.client.Minecraft;

/** The bridge's client init, shared by the loader entry classes once they filled {@link Loaders}. */
public final class BridgeInit {
    public static final String TEST_PROPERTY = "mcr.bridge.test";
    /** Client ticks after loading before {@code crash-in-tick} or {@code hang-in-tick} fires. */
    public static final String TEST_TICKS_PROPERTY = "mcr.bridge.test.ticks";
    private static final String TEST = System.getProperty(TEST_PROPERTY, "");
    private static final int TEST_TICKS = Integer.getInteger(TEST_TICKS_PROPERTY, 60);
    private static int ticks;

    private BridgeInit() {}

    /** The JVM tests' failures after loading, under either loader: a crash through vanilla's handling, or a hang. */
    public static void onTick(Minecraft minecraft) {
        if (TEST.isEmpty() || !minecraft.isGameLoadFinished() || minecraft.gui.overlay() != null) {
            return;
        }
        if (++ticks != TEST_TICKS) {
            return;
        }
        switch (TEST) {
            case "crash-in-tick" -> throw new IllegalStateException("mcr_bridge test crash in a client tick");
            case "hang-in-tick" -> {
                while (true) {
                    LockSupport.park();
                }
            }
            default -> {}
        }
    }

    public static void run(String entrypoints, Class<?> entry) {
        QuietRefusals.install();
        for (Loaders.Mod mod : Loaders.mods()) {
            Bridge.report(BridgeAbi.REPORT_MOD_LOADED, mod.id() + "\t" + mod.version());
        }
        Bridge.info("loader", Loaders.name());
        Bridge.info("client_entrypoints", entrypoints);
        Bridge.info("bridge_loader", entry.getClassLoader().getClass().getName());
        Bridge.info("boot_loader", Native.class.getClassLoader().getClass().getName());
        switch (TEST) {
            case "throw-in-init" -> throw new IllegalStateException("mcr_bridge test failure in client init");
            case "exit-in-init" -> System.exit(7);
            default -> {}
        }
        Bridge.report(BridgeAbi.REPORT_INIT, "mcr_bridge");
    }
}
