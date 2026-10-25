package dev.mcr.oracle.item;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import com.mojang.serialization.MapCodec;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import dev.mcr.oracle.model.VanillaModels;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.IntList;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.reflect.Field;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.client.model.geom.EntityModelSet;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.item.ClientItem;
import net.minecraft.client.renderer.item.CompositeModel;
import net.minecraft.client.renderer.item.ConditionalItemModel;
import net.minecraft.client.renderer.item.CuboidItemModelWrapper;
import net.minecraft.client.renderer.item.ItemModel;
import net.minecraft.client.renderer.item.ItemModelResolver;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.client.renderer.item.RangeSelectItemModel;
import net.minecraft.client.renderer.item.SelectItemModel;
import net.minecraft.client.renderer.item.SpecialModelWrapper;
import net.minecraft.client.renderer.special.SpecialModelRenderers;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.resources.LegacyStuffWrapper;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.model.ClientItemInfoLoader;
import net.minecraft.client.resources.model.ModelBakery;
import net.minecraft.client.resources.model.ModelDiscovery;
import net.minecraft.client.resources.model.ResolvableModel;
import net.minecraft.client.resources.model.ResolvedModel;
import net.minecraft.client.resources.model.UnbakedModel;
import net.minecraft.client.resources.model.cuboid.ItemModelGenerator;
import net.minecraft.client.resources.model.cuboid.MissingCuboidModel;
import net.minecraft.client.resources.model.sprite.MaterialBaker;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponents;
import net.minecraft.data.AtlasIds;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.world.entity.ItemOwner;
import net.minecraft.world.item.ItemDisplayContext;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GrassColor;
import net.minecraft.world.level.Level;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;

/**
 * Item model selection over generated stacks: vanilla's item definitions (and per-case generated ones) baked by
 * {@link ModelBakery}, updated into a render state with no level and no owner; see mcr-parity's item_model.rs.
 */
public final class ItemModelHarness implements Harness {
    static final double STEP = 1.0 / 1024.0;

    private final RegistryAccess.Frozen registries;
    private final VanillaItems items;
    private final Map<Identifier, ItemModel> models;
    private final ItemModelResolver resolver;

    @SuppressWarnings("deprecation")
    public ItemModelHarness(Boot boot) {
        registries = Boot.registryAccess();
        try {
            GrassColor.init(LegacyStuffWrapper.getPixels(boot.clientResources(), Identifier.withDefaultNamespace("textures/colormap/grass.png")));
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        items = new VanillaItems(boot.clientResources());
        models = new HashMap<>(items.bake(items.vanilla));
        resolver = new ItemModelResolver(null) {
            @Override
            public void appendItemLayers(ItemStackRenderState output, ItemStack item, ItemDisplayContext displayContext,
                    Level level, ItemOwner owner, int seed) {
                Identifier id = item.get(DataComponents.ITEM_MODEL);
                if (id != null) {
                    models.getOrDefault(id, items.missing()).update(output, item, this, displayContext, (ClientLevel) null, owner, seed);
                }
            }
        };
    }

    /** Vanilla's resources and definitions, baked with tags marking which leaf drew each layer. */
    static final class VanillaItems {
        final Map<Identifier, ClientItem> vanilla;
        private final MaterialBaker materials;
        private final Map<Identifier, ResolvedModel> resolved;
        private final ResolvedModel missingModel;
        private final EntityModelSet entityModels = EntityModelSet.vanilla();
        private ItemModel missing;

        @SuppressWarnings("deprecation")
        VanillaItems(ResourceManager resources) {
            List<TextureAtlasSprite> cells = new ArrayList<>();
            materials = new MaterialBaker(
                    VanillaModels.atlas(resources, AtlasIds.BLOCKS,
                            TextureAtlas.LOCATION_BLOCKS, cells),
                    VanillaModels.atlas(resources, AtlasIds.ITEMS,
                            TextureAtlas.LOCATION_ITEMS, cells));
            Map<Identifier, UnbakedModel> unbaked = VanillaModels.unbakedModels(resources);
            vanilla = ClientItemInfoLoader.scheduleLoad(resources, Runnable::run).join().contents();
            ModelDiscovery discovery = new ModelDiscovery(
                    unbaked, MissingCuboidModel.missingModel());
            discovery.addSpecialModel(ItemModelGenerator.GENERATED_ITEM_MODEL_ID,
                    new ItemModelGenerator());
            // Every model is a root, so a generated definition may name any of them.
            discovery.addRoot(resolver -> unbaked.keySet().forEach(resolver::markDependency));
            resolved = discovery.resolve();
            missingModel = discovery.missingModel();
        }

        Map<Identifier, ItemModel> bake(Map<Identifier, ClientItem> infos) {
            Map<Identifier, ClientItem> wrapped = new HashMap<>();
            infos.forEach((id, info) -> wrapped.put(id, new ClientItem(wrap(info.model()), info.properties(), info.registrySwapper())));
            ModelBakery bakery = new ModelBakery(entityModels, null, null, Map.of(), wrapped, resolved, missingModel);
            ModelBakery.BakingResult result = bakery.bakeModels(materials, Runnable::run).join();
            missing = result.missingModels().item();
            return result.itemStackModels();
        }

        ItemModel missing() {
            return missing;
        }
    }

    /** Marks the layers a leaf adds, since a baked cuboid or special model no longer knows its definition. */
    record Tagged(ItemModel.Unbaked inner, JsonObject tag) implements ItemModel.Unbaked {
        @Override
        public MapCodec<? extends ItemModel.Unbaked> type() {
            return inner.type();
        }

        @Override
        public void resolveDependencies(ResolvableModel.Resolver resolver) {
            inner.resolveDependencies(resolver);
        }

        @Override
        public ItemModel bake(ItemModel.BakingContext context, Matrix4fc transformation) {
            ItemModel baked = inner.bake(context, transformation);
            return (output, item, resolver, displayContext, level, owner, seed) -> {
                RecordingState state = (RecordingState) output;
                state.tags.push(tag);
                try {
                    baked.update(output, item, resolver, displayContext, level, owner, seed);
                } finally {
                    state.tags.pop();
                }
            };
        }
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    static ItemModel.Unbaked wrap(ItemModel.Unbaked model) {
        return switch (model) {
            case CuboidItemModelWrapper.Unbaked c -> {
                JsonObject tag = new JsonObject();
                tag.addProperty("model", c.model().toString());
                yield new Tagged(c, tag);
            }
            case SpecialModelWrapper.Unbaked s -> {
                JsonObject tag = new JsonObject();
                tag.add("special", SpecialModelRenderers.CODEC.encodeStart(JsonOps.INSTANCE, s.specialModel()).getOrThrow());
                tag.addProperty("base", s.base().toString());
                yield new Tagged(s, tag);
            }
            case CompositeModel.Unbaked c -> new CompositeModel.Unbaked(c.models().stream().map(ItemModelHarness::wrap).toList(), c.transformation());
            case ConditionalItemModel.Unbaked c -> new ConditionalItemModel.Unbaked(c.transformation(), c.property(), wrap(c.onTrue()), wrap(c.onFalse()));
            case SelectItemModel.Unbaked s -> {
                SelectItemModel.UnbakedSwitch sw = s.unbakedSwitch();
                List<SelectItemModel.SwitchCase> cases = ((List<SelectItemModel.SwitchCase>) sw.cases()).stream()
                        .map(c -> new SelectItemModel.SwitchCase(c.values(), wrap(c.model())))
                        .toList();
                yield new SelectItemModel.Unbaked(s.transformation(), new SelectItemModel.UnbakedSwitch(sw.property(), cases),
                        s.fallback().map(ItemModelHarness::wrap));
            }
            case RangeSelectItemModel.Unbaked r -> new RangeSelectItemModel.Unbaked(r.transformation(), r.property(), r.scale(),
                    r.entries().stream().map(e -> new RangeSelectItemModel.Entry(e.threshold(), wrap(e.model()))).toList(),
                    r.fallback().map(ItemModelHarness::wrap));
            default -> model;
        };
    }

    /** A render state that remembers which leaf each layer came from; a layer outside any leaf is the missing model. */
    static final class RecordingState extends ItemStackRenderState {
        final Deque<JsonObject> tags = new ArrayDeque<>();
        final List<JsonObject> layerTags = new ArrayList<>();
        final List<ItemStackRenderState.LayerRenderState> layers = new ArrayList<>();

        @Override
        public ItemStackRenderState.LayerRenderState newLayer() {
            JsonObject tag = tags.peek();
            if (tag == null) {
                tag = new JsonObject();
                tag.addProperty("missing", true);
            }
            layerTags.add(tag);
            ItemStackRenderState.LayerRenderState layer = super.newLayer();
            layers.add(layer);
            return layer;
        }
    }

    private static Object field(Object owner, String name) {
        try {
            Field f = ItemStackRenderState.LayerRenderState.class.getDeclaredField(name);
            f.setAccessible(true);
            return f.get(owner);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static JsonArray matrix(Matrix4fc m) {
        JsonArray out = new JsonArray();
        for (int row = 0; row < 4; row++) {
            for (int col = 0; col < 4; col++) {
                out.add(Harness.round(m.get(col, row), STEP));
            }
        }
        return out;
    }

    private JsonObject layer(JsonObject tag, ItemStackRenderState.LayerRenderState layer) {
        JsonObject out = tag.deepCopy();
        JsonArray tints = new JsonArray();
        if (field(layer, "tintLayers") instanceof IntList list) {
            list.forEach(t -> tints.add(String.format("#%08X", t)));
        }
        out.add("tints", tints);
        out.add("transform", matrix((Matrix4f) field(layer, "localTransform")));
        out.addProperty("foil", field(layer, "foilType") != ItemStackRenderState.FoilType.NONE);
        return out;
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        ItemStack stack;
        try {
            byte[] bytes = HexFormat.of().parseHex(testCase.get("stack").getAsString());
            RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(bytes), registries);
            stack = ItemStack.OPTIONAL_STREAM_CODEC.decode(buf);
        } catch (RuntimeException e) {
            JsonObject error = new JsonObject();
            System.err.println("[item_model] stack: " + e);
            error.addProperty("error", "decode");
            return error;
        }
        if (testCase.has("definition")) {
            Identifier id = Identifier.parse(testCase.get("def_id").getAsString());
            DataResult<ClientItem> result = ClientItem.CODEC.parse(RegistryOps.create(JsonOps.INSTANCE, registries), testCase.get("definition"));
            result.error().ifPresent(e -> System.err.println("[item_model] " + id + ": " + e.message()));
            Optional<ClientItem> parsed = result.result();
            models.remove(id);
            parsed.ifPresent(info -> models.putAll(items.bake(Map.of(id, info))));
        }
        ItemDisplayContext display = ItemDisplayContext.CODEC.parse(JsonOps.INSTANCE, testCase.get("display")).getOrThrow();
        RecordingState state = new RecordingState();
        if (!stack.isEmpty()) {
            resolver.appendItemLayers(state, stack, display, null, null, 0);
        }
        JsonArray layers = new JsonArray();
        for (int i = 0; i < state.layers.size(); i++) {
            layers.add(layer(state.layerTags.get(i), state.layers.get(i)));
        }
        JsonObject out = new JsonObject();
        out.add("layers", layers);
        return out;
    }
}
