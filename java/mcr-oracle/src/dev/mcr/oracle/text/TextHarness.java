package dev.mcr.oracle.text;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import java.util.List;
import java.util.Map;
import net.minecraft.client.resources.language.ClientLanguage;
import net.minecraft.locale.Language;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.ClickEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.Style;
import net.minecraft.util.StringDecomposer;

/** Component parsing, {@code getString} and the styled code points vanilla draws; see mcr-parity's text.rs. */
public final class TextHarness implements Harness {
    public TextHarness(Boot boot) {
        Language.inject(ClientLanguage.loadFrom(boot.clientResources(), List.of("en_us"), false));
    }

    /** Integers become IntTag, other numbers DoubleTag, booleans ByteTag, like the probe. */
    static Tag toTag(JsonElement json) {
        if (json.isJsonObject()) {
            CompoundTag tag = new CompoundTag();
            for (Map.Entry<String, JsonElement> e : json.getAsJsonObject().entrySet()) {
                tag.put(e.getKey(), toTag(e.getValue()));
            }
            return tag;
        }
        if (json.isJsonArray()) {
            return NbtOps.INSTANCE.createList(json.getAsJsonArray().asList().stream().map(TextHarness::toTag));
        }
        JsonPrimitive p = json.getAsJsonPrimitive();
        if (p.isBoolean()) {
            return NbtOps.INSTANCE.createBoolean(p.getAsBoolean());
        }
        if (p.isNumber()) {
            String raw = p.getAsString();
            if (raw.contains(".") || raw.contains("e") || raw.contains("E")) {
                return NbtOps.INSTANCE.createDouble(p.getAsDouble());
            }
            long value = p.getAsLong();
            return value == (int) value ? NbtOps.INSTANCE.createInt((int) value) : NbtOps.INSTANCE.createLong(value);
        }
        return NbtOps.INSTANCE.createString(p.getAsString());
    }

    private static JsonElement nullable(String s) {
        return s == null ? JsonNull.INSTANCE : new JsonPrimitive(s);
    }

    private static String click(ClickEvent event) {
        if (event == null) {
            return null;
        }
        String name = event.action().getSerializedName();
        return switch (event) {
            case ClickEvent.RunCommand r -> name + ":" + r.command();
            case ClickEvent.SuggestCommand s -> name + ":" + s.command();
            case ClickEvent.CopyToClipboard c -> name + ":" + c.value();
            default -> event.toString();
        };
    }

    public static JsonObject style(Style style) {
        JsonObject s = new JsonObject();
        s.add("color", style.getColor() == null ? JsonNull.INSTANCE
                : new JsonPrimitive(String.format("#%06X", style.getColor().getValue())));
        s.add("shadow", style.getShadowColor() == null ? JsonNull.INSTANCE
                : new JsonPrimitive(String.format("#%08X", style.getShadowColor())));
        s.addProperty("bold", style.isBold());
        s.addProperty("italic", style.isItalic());
        s.addProperty("underlined", style.isUnderlined());
        s.addProperty("strikethrough", style.isStrikethrough());
        s.addProperty("obfuscated", style.isObfuscated());
        s.addProperty("font", style.getFont() instanceof FontDescription.Resource r ? r.id().toString() : style.getFont().toString());
        s.add("click", nullable(click(style.getClickEvent())));
        s.add("insertion", nullable(style.getInsertion()));
        return s;
    }

    /** The code points vanilla draws for {@code component}, adjacent ones of equal style merged into runs. */
    public static JsonArray runs(Component component) {
        return runs(component, false);
    }

    /** With {@code visibleOnly}, a plain space (no underline, strikethrough or obfuscation) drops its colour and shadow. */
    public static JsonArray runs(Component component, boolean visibleOnly) {
        JsonArray runs = new JsonArray();
        StringDecomposer.iterateFormatted(component, Style.EMPTY, (position, style, codepoint) -> {
            JsonObject s = style(style);
            if (visibleOnly && codepoint == ' ' && !style.isUnderlined() && !style.isStrikethrough() && !style.isObfuscated()) {
                s.add("color", JsonNull.INSTANCE);
                s.add("shadow", JsonNull.INSTANCE);
            }
            JsonObject last = runs.isEmpty() ? null : runs.get(runs.size() - 1).getAsJsonObject();
            if (last != null) {
                JsonObject lastStyle = last.deepCopy();
                lastStyle.remove("text");
                if (lastStyle.equals(s)) {
                    last.addProperty("text", last.get("text").getAsString() + Character.toString(codepoint));
                    return true;
                }
            }
            s.addProperty("text", Character.toString(codepoint));
            runs.add(s);
            return true;
        });
        return runs;
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        Component component;
        try {
            component = ComponentSerialization.CODEC.parse(NbtOps.INSTANCE, toTag(testCase.get("component"))).getOrThrow();
        } catch (IllegalStateException e) {
            JsonObject error = new JsonObject();
            error.addProperty("error", e.getMessage());
            return error;
        }
        JsonArray runs = runs(component);
        JsonObject out = new JsonObject();
        out.addProperty("plain", component.getString());
        out.add("runs", runs);
        return out;
    }
}
