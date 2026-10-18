package dev.mcr.bridge;

import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.MethodNode;
import org.spongepowered.asm.mixin.MixinEnvironment;
import org.spongepowered.asm.mixin.Mixins;
import org.spongepowered.asm.mixin.extensibility.IMixinConfig;
import org.spongepowered.asm.mixin.extensibility.IMixinConfigPlugin;
import org.spongepowered.asm.mixin.extensibility.IMixinInfo;
import org.spongepowered.asm.mixin.transformer.Config;
import org.spongepowered.asm.mixin.transformer.IMixinTransformer;
import org.spongepowered.asm.mixin.transformer.ext.Extensions;
import org.spongepowered.asm.mixin.transformer.ext.IExtension;

/** Loads, never initializes, the classes the last session with this mod set loaded (design: mod-jvm-preload). */
public final class McrPreload {
    public static final String PROPERTY = "mcr.preload";
    public static final int MAIN = 0;
    public static final int GAME_LOOP = 1;
    public static final int TITLE = 2;
    public static final int LOGIN = 3;
    private static final int MILESTONES = 4;
    private static final String HEADER = "mcr-jvm-preload 1";
    private static final int MAX_CLASSES = 200_000;
    private static final int MAX_NAME = 512;
    private static final String APPLY_DESC = "(Ljava/lang/String;Lorg/objectweb/asm/tree/ClassNode;Ljava/lang/String;"
            + "Lorg/spongepowered/asm/mixin/extensibility/IMixinInfo;)V";

    private static final Object LOCK = new Object();
    private static int reached = -1;
    private static volatile boolean stop;

    static final class Mark0 {}

    static final class Mark1 {}

    static final class Mark2 {}

    static final class Mark3 {}

    private McrPreload() {}

    /** Called when this session reaches milestone {@code k}; reaching one implies every earlier one. */
    public static void milestone(int k) {
        synchronized (LOCK) {
            if (k <= reached) {
                return;
            }
            ClassLoader loader = McrPreload.class.getClassLoader();
            for (int m = reached + 1; m <= k; m++) {
                try {
                    Class.forName(McrPreload.class.getName() + "$Mark" + m, false, loader);
                } catch (ClassNotFoundException e) {
                    throw new IllegalStateException(e);
                }
            }
            boolean first = reached < 0;
            reached = k;
            LOCK.notifyAll();
            if (first) {
                start();
            }
        }
    }

    /** Stops preloading for this session, e.g. at quit. */
    public static void cancel() {
        stop = true;
        synchronized (LOCK) {
            LOCK.notifyAll();
        }
    }

    private static void start() {
        String list = System.getProperty(PROPERTY, "");
        if (list.isEmpty()) {
            return;
        }
        if (Mixins.getUnvisitedCount() != 0) {
            Bridge.info("preload_stopped", "mixin configs still unselected at the game's main");
            return;
        }
        Thread.ofPlatform().daemon().name("mcr-preload").start(() -> run(Path.of(list)));
    }

    private static void run(Path file) {
        long started = System.nanoTime();
        List<List<String>> segments;
        try {
            segments = read(file);
        } catch (IOException | RuntimeException e) {
            Bridge.info("preload_stopped", "unreadable class list: " + e);
            return;
        }
        GameClasses classes = GameClasses.current();
        if (classes.processorPackages == null) {
            Bridge.info("preload_stopped", "FML's class processors are unreadable");
            return;
        }
        Exclusions exclusions = exclusions(classes.processorPackages);
        if (!exclusions.acting.isEmpty()) {
            Bridge.info("preload_plugins", "left to the game: " + exclusions.acting);
        }
        int configs = Mixins.getConfigs().size();
        Counts counts = new Counts();
        String stopped = null;
        for (int seg = 0; seg < segments.size() && stopped == null; seg++) {
            if (!await(seg)) {
                break;
            }
            if (Mixins.getUnvisitedCount() != 0 || Mixins.getConfigs().size() != configs) {
                stopped = "mixin configs changed after the game's main";
                break;
            }
            for (String name : segments.get(seg)) {
                if (stopped != null || stop) {
                    break;
                }
                if (exclusions.skips(name)) {
                    counts.skipped++;
                } else {
                    stopped = load(name, classes, counts);
                }
            }
            long ms = (System.nanoTime() - started) / 1_000_000;
            Bridge.info("preload", "segment " + seg + ": " + counts + " after " + ms + " ms");
        }
        if (stopped != null) {
            Bridge.info("preload_stopped", stopped);
        }
    }

    private static final class Counts {
        int loaded, present, missing, failed, skipped;

        @Override
        public String toString() {
            return "loaded " + loaded + ", already loaded " + present + ", missing " + missing + ", failed " + failed
                    + ", skipped " + skipped;
        }
    }

    /** Returns why preloading must stop, or null. */
    private static String load(String name, GameClasses classes, Counts counts) {
        if (classes.isLoaded(name)) {
            counts.present++;
            return null;
        }
        try {
            Class.forName(name, false, classes.loader);
            counts.loaded++;
        } catch (ClassNotFoundException e) {
            counts.missing++;
        } catch (LinkageError e) {
            counts.failed++;
        } catch (Throwable t) {
            return name + ": " + t;
        }
        return null;
    }

    private static boolean await(int seg) {
        synchronized (LOCK) {
            while (reached < seg && !stop) {
                try {
                    LOCK.wait();
                } catch (InterruptedException e) {
                    return false;
                }
            }
            return !stop;
        }
    }

    static List<List<String>> read(Path file) throws IOException {
        List<String> lines = Files.readAllLines(file, StandardCharsets.UTF_8);
        if (lines.isEmpty() || !lines.get(0).equals(HEADER)) {
            throw new IOException("not a " + HEADER + " list");
        }
        List<List<String>> segments = new ArrayList<>();
        int total = 0;
        for (String line : lines.subList(1, lines.size())) {
            if (line.startsWith("@")) {
                int seg = Integer.parseInt(line.substring(1));
                if (seg < segments.size() || seg >= MILESTONES) {
                    throw new IOException("segment " + seg + " out of order");
                }
                while (segments.size() <= seg) {
                    segments.add(new ArrayList<>());
                }
            } else if (!line.isEmpty() && line.length() <= MAX_NAME && !segments.isEmpty()) {
                if (++total > MAX_CLASSES) {
                    break;
                }
                segments.get(segments.size() - 1).add(line);
            }
        }
        return segments;
    }

    private static final String[] MIXIN_EXTENSIONS = {
        "org.spongepowered.", "com.llamalad7.mixinextras.", "com.bawnorton.mixinsquared."
    };

    /**
     * Left to the game: Mixin's extensions, which it loads while holding its lock (a preload holding one of their
     * class locks would deadlock), FML's class processors for the same reason, mixin packages, and targets and mod
     * packages of plugins acting in pre/postApply.
     */
    private record Exclusions(Set<String> targets, List<String> packages, List<String> acting) {
        boolean skips(String name) {
            if (targets.contains(name)) {
                return true;
            }
            for (String p : packages) {
                if (name.startsWith(p)) {
                    return true;
                }
            }
            return false;
        }
    }

    private static Exclusions exclusions(List<String> processorPackages) {
        Set<String> targets = new HashSet<>();
        List<String> packages = new ArrayList<>();
        List<String> acting = new ArrayList<>();
        packages.addAll(List.of(MIXIN_EXTENSIONS));
        packages.addAll(processorPackages);
        if (MixinEnvironment.getCurrentEnvironment().getActiveTransformer() instanceof IMixinTransformer transformer
                && transformer.getExtensions() instanceof Extensions extensions) {
            for (IExtension extension : extensions.getExtensions()) {
                String name = extension.getClass().getName();
                packages.add(name.substring(0, Math.max(name.lastIndexOf('.'), 0)) + ".");
            }
        }
        for (Config config : Mixins.getConfigs()) {
            IMixinConfig mixins = config.getConfig();
            String pkg = mixins.getMixinPackage();
            if (pkg != null && !pkg.isEmpty()) {
                packages.add(pkg.endsWith(".") ? pkg : pkg + ".");
            }
            IMixinConfigPlugin plugin = mixins.getPlugin();
            if (plugin == null || !(acts(plugin, "preApply") || acts(plugin, "postApply"))) {
                continue;
            }
            String name = plugin.getClass().getName();
            acting.add(name);
            String parent = name.substring(0, Math.max(name.lastIndexOf('.'), 0));
            int dot = parent.lastIndexOf('.');
            if (dot > 0 && parent.indexOf('.') < dot) {
                parent = parent.substring(0, dot);
            }
            if (!parent.isEmpty()) {
                packages.add(parent + ".");
            }
            for (String target : mixins.getTargets()) {
                targets.add(target.replace('/', '.'));
            }
        }
        return new Exclusions(targets, packages, acting);
    }

    private static boolean acts(IMixinConfigPlugin plugin, String method) {
        try {
            Method m = plugin.getClass()
                    .getMethod(method, String.class, ClassNode.class, String.class, IMixinInfo.class);
            Class<?> owner = m.getDeclaringClass();
            ClassLoader loader = owner.getClassLoader();
            if (loader == null) {
                return true;
            }
            try (InputStream in = loader.getResourceAsStream(owner.getName().replace('.', '/') + ".class")) {
                if (in == null) {
                    return true;
                }
                ClassNode node = new ClassNode();
                new ClassReader(in.readAllBytes()).accept(node, ClassReader.SKIP_DEBUG | ClassReader.SKIP_FRAMES);
                for (MethodNode mn : node.methods) {
                    if (mn.name.equals(method) && mn.desc.equals(APPLY_DESC)) {
                        for (AbstractInsnNode insn : mn.instructions) {
                            if (insn.getOpcode() >= 0 && insn.getOpcode() != Opcodes.RETURN) {
                                return true;
                            }
                        }
                        return false;
                    }
                }
                return true;
            }
        } catch (ReflectiveOperationException | IOException | RuntimeException | LinkageError e) {
            return true;
        }
    }
}
