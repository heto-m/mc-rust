package dev.mcr.oracle.model;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.List;
import java.util.Map;
import net.minecraft.client.model.geom.builders.UVPair;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart;
import net.minecraft.client.renderer.block.dispatch.SingleVariant;
import net.minecraft.client.renderer.block.dispatch.WeightedVariants;
import net.minecraft.client.renderer.block.dispatch.multipart.MultiPartModel;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.util.random.Weighted;
import net.minecraft.util.random.WeightedList;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import org.joml.Vector3fc;

/** Baked quads per block state, by variant and cull face; see mcr-parity's block_model.rs. */
public final class BlockModelHarness implements Harness {
    static final double STEP = 1.0 / 1024.0;
    private static final Direction[] BUCKETS = {null, Direction.DOWN, Direction.UP, Direction.NORTH, Direction.SOUTH,
            Direction.WEST, Direction.EAST};

    private final VanillaModels models;

    public BlockModelHarness(Boot boot) {
        this.models = new VanillaModels(boot.clientResources());
    }

    private static <T extends Comparable<T>> BlockState with(BlockState state, Property<T> property, String value) {
        return property.getValue(value).map(v -> state.setValue(property, v))
                .orElseThrow(() -> new IllegalArgumentException(property.getName() + "=" + value));
    }

    public static BlockState state(JsonObject testCase) {
        Block block = BuiltInRegistries.BLOCK.getValue(Identifier.parse(testCase.get("block").getAsString()));
        BlockState state = block.defaultBlockState();
        for (Map.Entry<String, JsonElement> e : testCase.getAsJsonObject("props").entrySet()) {
            Property<?> property = block.getStateDefinition().getProperty(e.getKey());
            if (property == null) {
                throw new IllegalArgumentException("no property " + e.getKey() + " on " + block);
            }
            state = with(state, property, e.getValue().getAsString());
        }
        return state;
    }

    private static Object field(Object owner, Class<?> type, String name) throws ReflectiveOperationException {
        Field f = type.getDeclaredField(name);
        f.setAccessible(true);
        return f.get(owner);
    }

    @SuppressWarnings("unchecked")
    private static void variants(BlockStateModel model, int weight, JsonArray out) throws ReflectiveOperationException {
        switch (model) {
            case SingleVariant single -> out.add(variant((BlockStateModelPart) field(single, SingleVariant.class, "model"), weight));
            case WeightedVariants weighted -> {
                WeightedList<BlockStateModel> list = (WeightedList<BlockStateModel>) field(weighted, WeightedVariants.class, "list");
                for (Weighted<BlockStateModel> entry : list.unwrap()) {
                    variants(entry.value(), entry.weight(), out);
                }
            }
            default -> throw new IllegalStateException("unexpected model " + model.getClass());
        }
    }

    @SuppressWarnings("unchecked")
    private static JsonArray parts(BlockStateModel model, BlockState state) throws ReflectiveOperationException {
        JsonArray parts = new JsonArray();
        if (model instanceof MultiPartModel multipart) {
            Object shared = field(multipart, MultiPartModel.class, "shared");
            Method select = shared.getClass().getDeclaredMethod("selectModels", BlockState.class);
            select.setAccessible(true);
            for (BlockStateModel selected : (List<BlockStateModel>) select.invoke(shared, state)) {
                JsonArray group = new JsonArray();
                variants(selected, 1, group);
                parts.add(group);
            }
        } else {
            JsonArray group = new JsonArray();
            variants(model, 1, group);
            parts.add(group);
        }
        return parts;
    }

    static double local(float atlas, float min, float max) {
        return Harness.round((atlas - min) / (max - min), STEP);
    }

    static JsonObject quad(BakedQuad quad) {
        BakedQuad.MaterialInfo info = quad.materialInfo();
        TextureAtlasSprite sprite = info.sprite();
        JsonObject q = new JsonObject();
        q.addProperty("face", quad.direction().getName());
        q.addProperty("sprite", sprite.contents().name().toString());
        q.addProperty("tint", info.tintIndex());
        Direction shade = info.shadeDirectionOverride() != null ? info.shadeDirectionOverride() : quad.direction();
        q.addProperty("shade", shade.getName());
        q.addProperty("emission", info.lightEmission());
        JsonArray vertices = new JsonArray();
        for (int i = 0; i < 4; i++) {
            Vector3fc p = quad.position(i);
            long uv = quad.packedUV(i);
            JsonArray v = new JsonArray();
            v.add(Harness.round(p.x(), STEP));
            v.add(Harness.round(p.y(), STEP));
            v.add(Harness.round(p.z(), STEP));
            v.add(local(UVPair.unpackU(uv), sprite.getU0(), sprite.getU1()));
            v.add(local(UVPair.unpackV(uv), sprite.getV0(), sprite.getV1()));
            vertices.add(v);
        }
        q.add("v", vertices);
        return q;
    }

    private static JsonObject variant(BlockStateModelPart part, int weight) {
        JsonObject v = new JsonObject();
        v.addProperty("weight", weight);
        v.addProperty("ao", part.useAmbientOcclusion());
        v.addProperty("particle", part.particleMaterial().sprite().contents().name().toString());
        JsonObject quads = new JsonObject();
        for (Direction bucket : BUCKETS) {
            List<BakedQuad> list = part.getQuads(bucket);
            if (!list.isEmpty()) {
                JsonArray array = new JsonArray();
                list.forEach(q -> array.add(quad(q)));
                quads.add(bucket == null ? "none" : bucket.getName(), array);
            }
        }
        v.add("quads", quads);
        return v;
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        BlockState state = state(testCase);
        JsonObject out = new JsonObject();
        if (!models.hasModel(state)) {
            out.addProperty("missing", true);
            return out;
        }
        try {
            out.add("parts", parts(models.blocks.get(state), state));
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
        return out;
    }
}
