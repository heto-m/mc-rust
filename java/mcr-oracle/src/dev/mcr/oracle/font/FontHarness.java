package dev.mcr.oracle.font;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.StringSplitter;
import net.minecraft.client.gui.Font;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.FormattedText;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.network.chat.Style;
import net.minecraft.resources.Identifier;
import net.minecraft.util.StringDecomposer;

/** Glyph advances, {@code Font.width}, {@code StringSplitter.splitLines} and chat wrapping; see mcr-parity's font.rs. */
public final class FontHarness implements Harness {
    private static final double STEP = 1.0 / 1024.0;
    private static final int CHAT_MIN = 40;
    private static final int CHAT_MAX = 320;

    private final Font font;
    private final StringSplitter splitter;

    public FontHarness(Boot boot) {
        this.font = VanillaFonts.load(boot.clientResources());
        this.splitter = font.getSplitter();
    }

    private static String fontName(Style style) {
        return style.getFont() instanceof FontDescription.Resource r ? r.id().toString() : style.getFont().toString();
    }

    private static String visible(FormattedText text) {
        StringBuilder out = new StringBuilder();
        StringDecomposer.iterateFormatted(text, Style.EMPTY, (position, style, codepoint) -> {
            out.appendCodePoint(codepoint);
            return true;
        });
        return out.toString();
    }

    private float advance(int codepoint, Style style) {
        return splitter.stringWidth(FormattedText.of(Character.toString(codepoint), style));
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        MutableComponent message = Component.empty();
        StringBuilder raw = new StringBuilder();
        for (JsonElement p : testCase.getAsJsonArray("parts")) {
            JsonObject part = p.getAsJsonObject();
            String text = part.get("text").getAsString();
            Style style = Style.EMPTY;
            if (part.get("bold").getAsBoolean()) {
                style = style.withBold(true);
            }
            JsonElement fontId = part.get("font");
            if (fontId != null && !fontId.isJsonNull()) {
                style = style.withFont(new FontDescription.Resource(Identifier.parse(fontId.getAsString())));
            }
            message.append(Component.literal(text).withStyle(style));
            raw.append(text);
        }
        int wrap = testCase.get("wrap").getAsInt();

        JsonArray advances = new JsonArray();
        Set<String> seen = new HashSet<>();
        StringDecomposer.iterateFormatted(message, Style.EMPTY, (position, style, codepoint) -> {
            String name = fontName(style);
            if (seen.add(name + "|" + codepoint)) {
                JsonObject a = new JsonObject();
                a.addProperty("cp", String.format("U+%04X", codepoint));
                a.addProperty("font", name);
                a.addProperty("normal", Harness.round(advance(codepoint, style.withBold(false)), STEP));
                a.addProperty("bold", Harness.round(advance(codepoint, style.withBold(true)), STEP));
                advances.add(a);
            }
            return true;
        });

        JsonArray lines = new JsonArray();
        List<String> chat = new ArrayList<>();
        splitter.splitLines(message, wrap, Style.EMPTY, (line, wrapped) -> {
            JsonObject l = new JsonObject();
            String text = visible(line);
            l.addProperty("text", text);
            l.addProperty("width", Harness.round(splitter.stringWidth(line), STEP));
            lines.add(l);
            // ComponentRenderUtils.wrapComponents (chatColors on) indents continuation lines by one space.
            chat.add(wrapped ? " " + text : text);
        });

        JsonObject out = new JsonObject();
        out.add("advances", advances);
        out.addProperty("width", font.width(raw.toString()));
        out.addProperty("styled_width", Harness.round(splitter.stringWidth(message), STEP));
        out.add("lines", lines);
        if (wrap >= CHAT_MIN && wrap <= CHAT_MAX) {
            JsonArray c = new JsonArray();
            if (chat.isEmpty()) {
                c.add("");
            } else {
                chat.forEach(c::add);
            }
            out.add("chat", c);
        }
        return out;
    }
}
