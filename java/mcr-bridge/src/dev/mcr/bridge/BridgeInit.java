package dev.mcr.bridge;

import dev.mcr.boot.Native;

/** The bridge's client init, shared by the loader entry classes once they filled {@link Loaders}. */
public final class BridgeInit {
    public static final String TEST_PROPERTY = "mcr.bridge.test";

    private BridgeInit() {}

    public static void run(String entrypoints, Class<?> entry) {
        QuietRefusals.install();
        for (Loaders.Mod mod : Loaders.mods()) {
            Bridge.report(BridgeAbi.REPORT_MOD_LOADED, mod.id() + "\t" + mod.version());
        }
        Bridge.info("loader", Loaders.name());
        Bridge.info("client_entrypoints", entrypoints);
        Bridge.info("bridge_loader", entry.getClassLoader().getClass().getName());
        Bridge.info("boot_loader", Native.class.getClassLoader().getClass().getName());
        switch (System.getProperty(TEST_PROPERTY, "")) {
            case "throw-in-init" -> throw new IllegalStateException("mcr_bridge test failure in client init");
            case "exit-in-init" -> System.exit(7);
            default -> {}
        }
        Bridge.report(BridgeAbi.REPORT_INIT, "mcr_bridge");
    }
}
