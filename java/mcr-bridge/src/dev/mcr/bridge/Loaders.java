package dev.mcr.bridge;

import java.nio.file.Path;
import java.util.List;
import java.util.function.Predicate;

/** What the loader that started the game tells the bridge, filled in by its entry class. */
public final class Loaders {
    public record Mod(String id, String version, String topLevel, List<Path> roots) {}

    private static volatile String name = "unknown";
    private static volatile List<Mod> mods = List.of();
    private static volatile Predicate<String> clientCommands = command -> false;

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

    /** The loader's own client-side command dispatch; it answers whether it ran the command. */
    public static void clientCommands(Predicate<String> run) {
        clientCommands = run;
    }

    public static boolean runClientCommand(String command) {
        return clientCommands.test(command);
    }
}
