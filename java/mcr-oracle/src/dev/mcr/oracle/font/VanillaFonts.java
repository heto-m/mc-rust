package dev.mcr.oracle.font;

import com.mojang.blaze3d.font.GlyphBitmap;
import com.mojang.blaze3d.font.GlyphInfo;
import com.mojang.blaze3d.font.GlyphProvider;
import dev.mcr.oracle.Boot;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Collections;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GlyphSource;
import net.minecraft.client.gui.font.AllMissingGlyphProvider;
import net.minecraft.client.gui.font.FontManager;
import net.minecraft.client.gui.font.FontOption;
import net.minecraft.client.gui.font.FontSet;
import net.minecraft.client.gui.font.GlyphStitcher;
import net.minecraft.client.gui.font.glyphs.BakedSheetGlyph;
import net.minecraft.client.gui.font.glyphs.EffectGlyph;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.resources.ResourceManager;

/** Vanilla's fonts without a GPU: FontManager's own provider loading, FontSets whose glyphs carry metrics only. */
final class VanillaFonts {
    private VanillaFonts() {
    }

    private static final class MetricsStitcher extends GlyphStitcher {
        MetricsStitcher(Identifier id) {
            super(null, id);
        }

        @Override
        public BakedSheetGlyph stitch(GlyphInfo info, GlyphBitmap bitmap) {
            return new BakedSheetGlyph(info, null, null, 0, 0, 0, 0, 0, 0, 0, 0);
        }

        @Override
        public void reset() {
        }
    }

    private static FontSet fontSet(Identifier id, List<GlyphProvider.Conditional> providers, Set<FontOption> options) {
        FontSet set = new FontSet(new MetricsStitcher(id));
        set.reload(providers, options);
        return set;
    }

    @SuppressWarnings("unchecked")
    private static Map<Identifier, List<GlyphProvider.Conditional>> prepare(ResourceManager resources)
            throws ReflectiveOperationException {
        // FontManager's constructor bakes its missing font onto a GPU texture; prepare() touches no field.
        FontManager manager = Boot.allocate(FontManager.class);
        Method prepare = FontManager.class.getDeclaredMethod("prepare", ResourceManager.class, Executor.class);
        prepare.setAccessible(true);
        Executor direct = Runnable::run;
        Object preparation = ((CompletableFuture<?>) prepare.invoke(manager, resources, direct)).join();
        Method fontSets = preparation.getClass().getDeclaredMethod("fontSets");
        fontSets.setAccessible(true);
        return (Map<Identifier, List<GlyphProvider.Conditional>>) fontSets.invoke(preparation);
    }

    static Font load(ResourceManager resources, Set<FontOption> options) {
        Map<Identifier, FontSet> sets = new HashMap<>();
        try {
            for (Map.Entry<Identifier, List<GlyphProvider.Conditional>> e : prepare(resources).entrySet()) {
                List<GlyphProvider.Conditional> providers = new ArrayList<>(e.getValue());
                Collections.reverse(providers);
                sets.put(e.getKey(), fontSet(e.getKey(), providers, options));
            }
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("loading vanilla fonts", e);
        }
        if (!sets.containsKey(FontDescription.DEFAULT.id())) {
            throw new IllegalStateException("default font failed to load");
        }
        FontSet missing = fontSet(FontManager.MISSING_FONT,
                List.of(new GlyphProvider.Conditional(new AllMissingGlyphProvider(), FontOption.Filter.ALWAYS_PASS)), Set.of());
        return new Font(new Font.Provider() {
            @Override
            public GlyphSource glyphs(FontDescription font) {
                FontSet set = font instanceof FontDescription.Resource r ? sets.getOrDefault(r.id(), missing) : missing;
                return set.source(false);
            }

            @Override
            public EffectGlyph effect() {
                return sets.get(FontDescription.DEFAULT.id()).whiteGlyph();
            }
        });
    }

    static Font load(ResourceManager resources) {
        return load(resources, EnumSet.noneOf(FontOption.class));
    }
}
