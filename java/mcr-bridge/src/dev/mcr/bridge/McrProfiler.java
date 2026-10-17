package dev.mcr.bridge;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import jdk.jfr.consumer.RecordedEvent;
import jdk.jfr.consumer.RecordedFrame;
import jdk.jfr.consumer.RecordedStackTrace;
import jdk.jfr.consumer.RecordedThread;
import jdk.jfr.consumer.RecordingStream;
import net.fabricmc.loader.api.FabricLoader;
import net.fabricmc.loader.api.ModContainer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Attributes the render thread's time to mods (`-Dmcr.profile`, set by `MCR_MOD_PROFILE`): JFR samples
 * it every millisecond and each sample goes to the innermost mod frame on its stack, mixin handlers
 * included, or to Minecraft when there is none.
 */
public final class McrProfiler {
    public static final boolean ENABLED = Boolean.getBoolean("mcr.profile");
    private static final Logger LOGGER = LoggerFactory.getLogger("mcr-profile");
    private static final long REPORT_NANOS = 10_000_000_000L;
    private static final int TOP = 15;
    private static final String VANILLA = "minecraft";
    private static final String BRIDGE = "mcr_bridge";
    private static final String NATIVE = "(native: rust gpu calls)";
    private static final Set<String> PLATFORM = Set.of("minecraft", "java", "fabricloader", "mixinextras");

    private static volatile long renderThreadId = -1;
    private static final Map<String, String> owners = new ConcurrentHashMap<>();
    private static final Map<String, long[]> counts = new HashMap<>();
    private static final Map<String, String> topLevel = new HashMap<>();
    private static final List<Map.Entry<String, List<Path>>> roots = new ArrayList<>();
    private static long samples;
    private static long lastReport = System.nanoTime();

    /** Starts sampling `render` once; a no-op unless profiling is on. */
    public static void start(Thread render) {
        if (!ENABLED || renderThreadId >= 0) {
            return;
        }
        renderThreadId = render.threadId();
        for (ModContainer mod : FabricLoader.getInstance().getAllMods()) {
            ModContainer top = mod;
            while (top.getContainingMod().isPresent()) {
                top = top.getContainingMod().get();
            }
            String id = mod.getMetadata().getId();
            topLevel.put(id, top.getMetadata().getId());
            roots.add(Map.entry(id, mod.getRootPaths()));
        }
        Thread t = new Thread(McrProfiler::run, "mcr-profiler");
        t.setDaemon(true);
        t.start();
    }

    private static void run() {
        try (RecordingStream stream = new RecordingStream()) {
            stream.enable("jdk.ExecutionSample").withPeriod(Duration.ofMillis(1));
            stream.enable("jdk.NativeMethodSample").withPeriod(Duration.ofMillis(1));
            stream.onEvent("jdk.ExecutionSample", e -> sample(e, false));
            stream.onEvent("jdk.NativeMethodSample", e -> sample(e, true));
            LOGGER.info("sampling the render thread every millisecond");
            stream.start();
        } catch (Throwable t) {
            LOGGER.warn("render thread profiler stopped", t);
        }
    }

    private static void sample(RecordedEvent event, boolean inNative) {
        RecordedThread thread = event.getThread("sampledThread");
        RecordedStackTrace stack = event.getStackTrace();
        if (thread == null || thread.getJavaThreadId() != renderThreadId || stack == null) {
            return;
        }
        List<RecordedFrame> frames = stack.getFrames();
        if (inNative && idle(frames)) {
            return;
        }
        String owner = attribute(frames);
        if (inNative && (owner.equals(VANILLA) || owner.equals(BRIDGE))) {
            owner = NATIVE;
        }
        counts.computeIfAbsent(owner, k -> new long[1])[0]++;
        samples++;
        long now = System.nanoTime();
        if (now - lastReport >= REPORT_NANOS) {
            report(now);
        }
    }

    /** Blocked in the frame wait: Java has nothing to do until mcr posts the next frame. */
    private static boolean idle(List<RecordedFrame> frames) {
        for (RecordedFrame f : frames) {
            if (f.isJavaFrame() && f.getMethod().getType().getName().equals("dev.mcr.boot.Doorbell")) {
                return true;
            }
        }
        return false;
    }

    static String attribute(List<RecordedFrame> frames) {
        boolean bridge = false;
        for (RecordedFrame f : frames) {
            if (!f.isJavaFrame()) {
                continue;
            }
            String mod = handlerMod(f.getMethod().getName());
            if (mod == null) {
                mod = owner(f.getMethod().getType().getName());
            }
            if (mod == null || PLATFORM.contains(mod)) {
                continue;
            }
            if (mod.equals(BRIDGE)) {
                bridge = true;
                continue;
            }
            return mod;
        }
        return bridge ? BRIDGE : VANILLA;
    }

    /** Mixin merges handlers into the target class as `kind$xyz000$modid$name`. */
    static String handlerMod(String method) {
        int a = method.indexOf('$');
        if (a < 0) {
            return null;
        }
        int b = method.indexOf('$', a + 1);
        int c = b < 0 ? -1 : method.indexOf('$', b + 1);
        if (c < 0) {
            return null;
        }
        String id = method.substring(b + 1, c);
        return topLevel.get(id);
    }

    private static String owner(String className) {
        int lambda = className.indexOf("$$");
        String name = lambda < 0 ? className : className.substring(0, lambda);
        String found = owners.get(name);
        if (found != null) {
            return found.isEmpty() ? null : found;
        }
        String resource = name.replace('.', '/') + ".class";
        String owner = "";
        search:
        for (Map.Entry<String, List<Path>> mod : roots) {
            for (Path root : mod.getValue()) {
                try {
                    if (Files.exists(root.resolve(resource))) {
                        owner = topLevel.getOrDefault(mod.getKey(), mod.getKey());
                        break search;
                    }
                } catch (RuntimeException ignored) {
                    // A root that cannot resolve this name does not own it.
                }
            }
        }
        owners.put(name, owner);
        return owner.isEmpty() ? null : owner;
    }

    private static void report(long now) {
        double seconds = (now - lastReport) / 1e9;
        lastReport = now;
        List<Map.Entry<String, long[]>> sorted = new ArrayList<>(counts.entrySet());
        sorted.sort((x, y) -> Long.compare(y.getValue()[0], x.getValue()[0]));
        StringBuilder line = new StringBuilder();
        line.append(String.format("render thread busy %.0f%% over %.0fs:", samples / (seconds * 10.0), seconds));
        for (Map.Entry<String, long[]> e : sorted.subList(0, Math.min(TOP, sorted.size()))) {
            line.append(String.format(" %s %.1f%%", e.getKey(), 100.0 * e.getValue()[0] / samples));
        }
        LOGGER.info(line.toString());
        counts.clear();
        samples = 0;
    }

    private McrProfiler() {}
}
