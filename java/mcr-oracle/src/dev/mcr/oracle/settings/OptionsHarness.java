package dev.mcr.oracle.settings;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.ToggleKeyMapping;
import net.minecraft.util.datafix.DataFixers;
import org.apache.logging.log4j.Level;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.core.LogEvent;
import org.apache.logging.log4j.core.LoggerContext;
import org.apache.logging.log4j.core.appender.AbstractAppender;
import org.apache.logging.log4j.core.config.Property;

/** Vanilla {@code Options} loading a generated {@code options.txt} and saving it again; see mcr-parity's options.rs. */
public final class OptionsHarness implements Harness {
    private static final Set<String> VANILLA_ONLY = Set.of("debugGuiScale", "glDebugVerbosity");

    private final Minecraft minecraft;
    private final Path dir;
    private final Path file;
    private final Set<String> known = new HashSet<>();
    private String loadError;

    /** {@code Options.load} swallows the exception that stops it; the log is the only trace. */
    private final class LoadErrors extends AbstractAppender {
        LoadErrors() {
            super("mcr-options", null, null, true, Property.EMPTY_ARRAY);
        }

        @Override
        public void append(LogEvent event) {
            if (event.getThrown() != null && "Failed to load options".equals(event.getMessage().getFormattedMessage())) {
                loadError = event.getThrown().getClass().getSimpleName();
            }
        }
    }

    public OptionsHarness(Boot boot) {
        try {
            minecraft = Boot.allocate(Minecraft.class);
            Field fixer = Minecraft.class.getDeclaredField("fixerUpper");
            fixer.setAccessible(true);
            fixer.set(minecraft, DataFixers.getDataFixer());
            // OptionInstance.set asks Minecraft.getInstance().isRunning(); an allocated client is not running.
            Field instance = Minecraft.class.getDeclaredField("instance");
            instance.setAccessible(true);
            instance.set(null, minecraft);
            dir = Files.createTempDirectory("mcr-options");
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("fake client", e);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        LoadErrors errors = new LoadErrors();
        errors.start();
        LoggerContext context = (LoggerContext) LogManager.getContext(false);
        context.getConfiguration().addAppender(errors);
        context.getConfiguration().getRootLogger().addAppender(errors, Level.ERROR, null);
        context.updateLoggers();
        file = dir.resolve("options.txt");
        dir.toFile().deleteOnExit();
        file.toFile().deleteOnExit();
        try {
            Files.deleteIfExists(file);
            new Options(minecraft, dir.toFile()).save();
            for (String line : Files.readAllLines(file, StandardCharsets.UTF_8)) {
                int colon = line.indexOf(':');
                if (colon >= 0) {
                    known.add(line.substring(0, colon));
                }
            }
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        known.remove("version");
        known.removeAll(VANILLA_ONLY);
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        Options options;
        String saved;
        try {
            if (testCase.get("text").isJsonNull()) {
                Files.deleteIfExists(file);
            } else {
                Files.writeString(file, testCase.get("text").getAsString(), StandardCharsets.UTF_8);
            }
            loadError = null;
            options = new Options(minecraft, dir.toFile());
            options.save();
            saved = Files.readString(file, StandardCharsets.UTF_8);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        JsonObject out = new JsonObject();
        JsonArray order = new JsonArray();
        for (String line : saved.split("\n", -1)) {
            int colon = line.indexOf(':');
            if (colon < 0) {
                continue;
            }
            String key = line.substring(0, colon);
            if (key.equals("version")) {
                continue;
            }
            if (known.contains(key)) {
                order.add(key);
            }
            out.addProperty(key, line.substring(colon + 1));
        }
        out.add("#order", order);
        if (loadError != null) {
            out.addProperty("#load_error", loadError);
        }
        out.add("#collisions", collisions(options));
        if (testCase.get("table").getAsBoolean()) {
            out.add("#table", table(options));
        }
        return out;
    }

    /** {@code KeyBindsList.KeyEntry.refreshEntry}'s duplicate check. */
    private static JsonObject collisions(Options options) {
        JsonObject out = new JsonObject();
        for (KeyMapping key : options.keyMappings) {
            if (key.isUnbound()) {
                continue;
            }
            List<String> names = new ArrayList<>();
            for (KeyMapping other : options.keyMappings) {
                if (other != key && key.same(other) && (!other.isDefault() || !key.isDefault())) {
                    names.add(other.getName());
                }
            }
            if (!names.isEmpty()) {
                Collections.sort(names);
                JsonArray list = new JsonArray();
                names.forEach(list::add);
                out.add(key.getName(), list);
            }
        }
        return out;
    }

    private static JsonArray table(Options options) {
        JsonArray out = new JsonArray();
        try {
            Field order = KeyMapping.class.getDeclaredField("order");
            order.setAccessible(true);
            for (KeyMapping key : options.keyMappings) {
                JsonObject row = new JsonObject();
                row.addProperty("name", key.getName());
                row.addProperty("default", key.getDefaultKey().getName());
                row.addProperty("category", key.getCategory().id().getPath());
                row.addProperty("order", order.getInt(key));
                row.addProperty("toggle", key instanceof ToggleKeyMapping);
                out.add(row);
            }
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("key mapping order", e);
        }
        return out;
    }
}
