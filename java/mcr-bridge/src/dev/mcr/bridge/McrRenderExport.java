package dev.mcr.bridge;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.color.block.BlockTintSource;
import net.minecraft.client.color.block.BlockTintSources;
import net.minecraft.client.model.geom.builders.UVPair;
import net.minecraft.client.renderer.block.FluidModel;
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.FlowingFluid;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import org.joml.Vector3fc;

/**
 * What Rust's terrain needs from Java's baked models: the fluid models (sprites, layer, tint) of every
 * fluid, and on request the quads of blocks Rust's own baker cannot bake (code-registered or
 * custom-loader models). Sent as JSON through {@code render_report}; Rust meshes the quads like its own.
 */
public final class McrRenderExport {
    private static final int MAX_BLOCKS = 4096;
    private static final int MAX_STATES = 4096;
    private static final int MAX_QUADS = 4096;
    private static final int MAX_REQUEST_BYTES = 1 << 20;
    private static final Set<String> pending = new LinkedHashSet<>();
    private static final Set<String> exported = new LinkedHashSet<>();
    private static boolean fluidsPending;
    private static boolean titleSeen;
    private static String sentPacks;
    private static int frames;

    private McrRenderExport() {}

    /** At the first title screen: the fluid models, once Java's resources are loaded. */
    public static void atTitle() {
        if (!titleSeen) {
            titleSeen = true;
            fluidsPending = true;
        }
    }

    /** {@code MSG_MODELS}: block names, one per line. */
    public static void request(MemorySegment payload) {
        if (payload.byteSize() > MAX_REQUEST_BYTES) return;
        String text = new String(payload.toArray(ValueLayout.JAVA_BYTE), StandardCharsets.UTF_8);
        for (String line : text.split("\n")) {
            if (!line.isEmpty() && pending.size() < MAX_BLOCKS) pending.add(line);
        }
    }

    /** Once per frame: answers what is pending when the models are loaded. */
    public static void frame(Minecraft minecraft) {
        if (!titleSeen || !minecraft.isGameLoadFinished() || minecraft.gui.overlay() != null) {
            return;
        }
        boolean due = !pending.isEmpty() || fluidsPending;
        if (!due && ++frames % 60 != 0) {
            return;
        }
        List<String> packs = packs(minecraft);
        String packsKey = String.join("\n", packs);
        if (sentPacks != null && !sentPacks.equals(packsKey)) {
            // The models and sprites Rust has were baked with the previous packs.
            fluidsPending = true;
            pending.addAll(exported);
        }
        if (pending.isEmpty() && !fluidsPending) {
            return;
        }
        sentPacks = packsKey;
        List<String> names = new ArrayList<>(pending);
        exported.addAll(pending);
        pending.clear();
        fluidsPending = false;
        long start = System.nanoTime();
        String json;
        try {
            json = build(minecraft, names, packs);
        } catch (RuntimeException | LinkageError e) {
            Bridge.info("render_report_failed", e);
            return;
        }
        byte[] bytes = json.getBytes(StandardCharsets.UTF_8);
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment segment = arena.allocate(Math.max(bytes.length, 1));
            MemorySegment.copy(bytes, 0, segment, ValueLayout.JAVA_BYTE, 0, bytes.length);
            Bridge.get().downcall(BridgeAbi.Entry.RENDER_REPORT).invokeExact(segment.address(), bytes.length);
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
        Bridge.info("render_report", names.size() + " blocks, " + bytes.length + " bytes in " + (System.nanoTime() - start) / 1_000_000 + " ms");
    }

    /** The selected resource packs of the resource pack folder, lowest priority first, as absolute paths. */
    private static List<String> packs(Minecraft minecraft) {
        List<String> out = new ArrayList<>();
        try {
            var dir = minecraft.getResourcePackDirectory().toAbsolutePath().normalize();
            for (var pack : minecraft.getResourcePackRepository().getSelectedPacks()) {
                String id = pack.getId();
                if (id.startsWith("file/")) {
                    var path = dir.resolve(id.substring("file/".length())).normalize();
                    if (path.startsWith(dir)) out.add(path.toString());
                }
            }
        } catch (RuntimeException e) {
            Bridge.info("render_report_packs", e);
        }
        return out;
    }

    static String build(Minecraft minecraft, List<String> names, List<String> packs) {
        Map<String, Integer> sprites = new HashMap<>();
        List<String> spriteList = new ArrayList<>();
        StringBuilder w = new StringBuilder(1 << 16);
        w.append("{\"format\":1,\"packs\":[");
        for (int i = 0; i < packs.size(); i++) {
            if (i > 0) w.append(',');
            w.append(McrRegistries.str(packs.get(i)));
        }
        w.append("],\"fluids\":[");
        var fluidModels = minecraft.getModelManager().getFluidStateModelSet();
        boolean first = true;
        for (Fluid fluid : BuiltInRegistries.FLUID) {
            if (fluid == Fluids.EMPTY) continue;
            FluidModel model;
            try {
                model = fluidModels.get(fluid.defaultFluidState());
            } catch (RuntimeException e) {
                continue;
            }
            if (!first) w.append(',');
            first = false;
            Fluid source = fluid instanceof FlowingFluid flowing ? flowing.getSource() : fluid;
            BlockState block = fluid.defaultFluidState().createLegacyBlock();
            w.append("{\"name\":").append(McrRegistries.str(BuiltInRegistries.FLUID.getKey(fluid).toString()))
                .append(",\"family\":").append(McrRegistries.str(BuiltInRegistries.FLUID.getKey(source).toString()))
                .append(",\"still\":").append(sprite(sprites, spriteList, model.stillMaterial().sprite()))
                .append(",\"flow\":").append(sprite(sprites, spriteList, model.flowingMaterial().sprite()))
                .append(",\"overlay\":").append(model.overlayMaterial() == null ? -1 : sprite(sprites, spriteList, model.overlayMaterial().sprite()))
                .append(",\"translucent\":").append(model.layer().translucent())
                .append(",\"tint\":").append(tint(model.tintSource(), block))
                .append('}');
        }
        w.append("],\"blocks\":[");
        var models = minecraft.getModelManager().getBlockStateModelSet();
        var colors = minecraft.getBlockColors();
        first = true;
        for (String name : names) {
            Identifier id = Identifier.tryParse(name);
            if (id == null) continue;
            Block block = BuiltInRegistries.BLOCK.getOptional(id).orElse(null);
            if (block == null) continue;
            if (!first) w.append(',');
            first = false;
            w.append("{\"name\":").append(McrRegistries.str(name)).append(",\"states\":[");
            var states = block.getStateDefinition().getPossibleStates();
            for (int s = 0; s < Math.min(states.size(), MAX_STATES); s++) {
                BlockState state = states.get(s);
                if (s > 0) w.append(',');
                List<BlockStateModelPart> parts = new ArrayList<>();
                if (state.getRenderShape() != RenderShape.INVISIBLE) {
                    models.get(state).collectParts(RandomSource.create(42L), parts);
                }
                boolean ao = parts.stream().allMatch(BlockStateModelPart::useAmbientOcclusion);
                w.append("{\"props\":").append(McrRegistries.str(props(state)))
                    .append(",\"ao\":").append(ao)
                    .append(",\"particle\":").append(sprite(sprites, spriteList, models.getParticleMaterial(state).sprite()))
                    .append(",\"tints\":[").append(tint(colors.getTintSource(state, 0), state))
                    .append(',').append(tint(colors.getTintSource(state, 1), state))
                    .append("],\"quads\":[");
                int count = 0;
                for (BlockStateModelPart part : parts) {
                    for (int d = 0; d <= 6; d++) {
                        Direction cull = d < 6 ? Direction.values()[d] : null;
                        for (BakedQuad quad : part.getQuads(cull)) {
                            if (count >= MAX_QUADS) break;
                            if (count++ > 0) w.append(',');
                            quad(w, quad, cull, sprites, spriteList);
                        }
                    }
                }
                w.append("]}");
            }
            w.append("]}");
        }
        w.append("],\"sprites\":[");
        for (int i = 0; i < spriteList.size(); i++) {
            if (i > 0) w.append(',');
            w.append(McrRegistries.str(spriteList.get(i)));
        }
        return w.append("]}").toString();
    }

    /** {@code [x,y,z,u,v] x4, direction, cull, tint index, shade direction, light emission, sprite, translucent}. */
    private static void quad(StringBuilder w, BakedQuad quad, Direction cull, Map<String, Integer> sprites, List<String> spriteList) {
        var info = quad.materialInfo();
        TextureAtlasSprite sprite = info.sprite();
        float du = sprite.getU1() - sprite.getU0();
        float dv = sprite.getV1() - sprite.getV0();
        w.append('[');
        for (int v = 0; v < 4; v++) {
            Vector3fc p = quad.position(v);
            long uv = quad.packedUV(v);
            float u = du == 0 ? 0 : (UVPair.unpackU(uv) - sprite.getU0()) / du;
            float t = dv == 0 ? 0 : (UVPair.unpackV(uv) - sprite.getV0()) / dv;
            w.append(num(p.x())).append(',').append(num(p.y())).append(',').append(num(p.z())).append(',')
                .append(num(u)).append(',').append(num(t)).append(',');
        }
        Direction shade = info.shadeDirectionOverride() != null ? info.shadeDirectionOverride() : quad.direction();
        w.append(quad.direction().ordinal()).append(',')
            .append(cull == null ? -1 : cull.ordinal()).append(',')
            .append(info.tintIndex()).append(',')
            .append(shade.ordinal()).append(',')
            .append(info.lightEmission()).append(',')
            .append(sprite(sprites, spriteList, sprite)).append(',')
            .append(info.layer().translucent() ? 1 : 0).append(']');
    }

    private static int sprite(Map<String, Integer> sprites, List<String> spriteList, TextureAtlasSprite sprite) {
        return sprites.computeIfAbsent(sprite.contents().name().toString(), n -> {
            spriteList.add(n);
            return spriteList.size() - 1;
        });
    }

    /** {@code "k=v,..."} sorted by property name, as Rust's tables list them. */
    private static String props(BlockState state) {
        List<Property<?>> props = new ArrayList<>(state.getProperties());
        props.sort(Comparator.comparing(Property::getName));
        StringBuilder b = new StringBuilder();
        for (Property<?> p : props) {
            if (!b.isEmpty()) b.append(',');
            b.append(p.getName()).append('=').append(value(state, p));
        }
        return b.toString();
    }

    private static <T extends Comparable<T>> String value(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    /** A biome tint by its vanilla source, else the source's colour for the state, else {@code "none"}. */
    private static String tint(BlockTintSource source, BlockState state) {
        if (source == null) return "\"none\"";
        Method made = source.getClass().getEnclosingMethod();
        if (made != null && made.getDeclaringClass() == BlockTintSources.class) {
            switch (made.getName()) {
                case "grass", "grassBlock", "doubleTallGrass", "sugarCane" -> {
                    return "\"grass\"";
                }
                case "foliage" -> {
                    return "\"foliage\"";
                }
                case "dryFoliage" -> {
                    return "\"dry_foliage\"";
                }
                case "water", "waterParticles" -> {
                    return "\"water\"";
                }
                default -> {}
            }
        }
        try {
            return Integer.toString(source.color(state) & 0xFFFFFF);
        } catch (RuntimeException e) {
            return "\"none\"";
        }
    }

    private static String num(float v) {
        return Float.isFinite(v) ? Float.toString(v) : "0";
    }
}
