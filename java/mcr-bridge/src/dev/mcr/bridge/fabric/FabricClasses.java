package dev.mcr.bridge.fabric;

import net.fabricmc.loader.impl.launch.FabricLauncher;
import net.fabricmc.loader.impl.launch.FabricLauncherBase;

/** Knot's class loader for the preloader; only loaded under Fabric. */
public final class FabricClasses {
    private static final FabricLauncher LAUNCHER = FabricLauncherBase.getLauncher();

    private FabricClasses() {}

    public static ClassLoader loader() {
        return LAUNCHER.getTargetClassLoader();
    }

    public static boolean isLoaded(String name) {
        return LAUNCHER.isClassLoaded(name);
    }
}
