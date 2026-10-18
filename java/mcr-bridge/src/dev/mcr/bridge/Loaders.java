package dev.mcr.bridge;

import java.nio.file.Path;
import java.util.List;

/** What the loader that started the game tells the bridge, filled in by its entry class. */
public final class Loaders {
    public record Mod(String id, String version, String topLevel, List<Path> roots) {}

    private static volatile String name = "unknown";
    private static volatile List<Mod> mods = List.of();

    private Loaders() {}

    public static void install(String loaderName, List<Mod> loaded) {
        name = loaderName;
        mods = List.copyOf(loaded);
    }

    public static String name() {
        return name;
    }

    public static List<Mod> mods() {
        return mods;
    }
}
