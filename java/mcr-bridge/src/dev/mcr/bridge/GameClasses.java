package dev.mcr.bridge;

import dev.mcr.bridge.fabric.FabricClasses;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Predicate;

/** The loader that defines the game's classes, for the preloader: Knot under Fabric, FML's transforming loader under NeoForge. */
final class GameClasses {
    private static final String FML_LOADER = "net.neoforged.fml.classloading.transformation.TransformingClassLoader";

    final ClassLoader loader;
    private final Predicate<String> loaded;
    /** Packages of FML's class processors, or null when they could not be read; empty under Fabric. */
    final List<String> processorPackages;

    private GameClasses(ClassLoader loader, Predicate<String> loaded, List<String> processorPackages) {
        this.loader = loader;
        this.loaded = loaded;
        this.processorPackages = processorPackages;
    }

    static GameClasses current() {
        ClassLoader loader = GameClasses.class.getClassLoader();
        if (!loader.getClass().getName().equals(FML_LOADER)) {
            return new GameClasses(FabricClasses.loader(), FabricClasses::isLoaded, List.of());
        }
        return new GameClasses(loader, loadedProbe(loader), processorPackages(loader));
    }

    boolean isLoaded(String name) {
        return loaded.test(name);
    }

    // ClassLoader.findLoadedClass is protected and java.base is not open to mods; FML's subclass wraps it privately.
    private static Predicate<String> loadedProbe(ClassLoader loader) {
        try {
            Method m = loader.getClass().getDeclaredMethod("getLoadedClass", String.class);
            m.setAccessible(true);
            return name -> {
                try {
                    return m.invoke(loader, name) != null;
                } catch (ReflectiveOperationException | RuntimeException e) {
                    return false;
                }
            };
        } catch (ReflectiveOperationException | RuntimeException e) {
            return name -> false;
        }
    }

    private static List<String> processorPackages(ClassLoader loader) {
        try {
            Object transformer = field(loader.getClass(), "classTransformer").get(loader);
            Object set = field(transformer.getClass(), "processors").get(transformer);
            List<String> packages = new ArrayList<>();
            for (Object processor : (List<?>) field(set.getClass(), "sortedProcessors").get(set)) {
                String name = processor.getClass().getName();
                packages.add(name.substring(0, Math.max(name.lastIndexOf('.'), 0)) + ".");
            }
            return packages;
        } catch (ReflectiveOperationException | RuntimeException e) {
            return null;
        }
    }

    private static Field field(Class<?> owner, String name) throws NoSuchFieldException {
        Field f = owner.getDeclaredField(name);
        f.setAccessible(true);
        return f;
    }
}
