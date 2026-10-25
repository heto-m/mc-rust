package dev.mcr.oracle.model;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import net.minecraft.client.model.geom.EntityModelSet;
import net.minecraft.client.renderer.block.BlockStateModelSet;
import net.minecraft.client.renderer.block.FluidStateModelSet;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.texture.SpriteContents;
import net.minecraft.client.renderer.texture.SpriteLoader;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.renderer.texture.atlas.SpriteResourceLoader;
import net.minecraft.client.renderer.texture.atlas.SpriteSource;
import net.minecraft.client.renderer.texture.atlas.SpriteSourceList;
import net.minecraft.client.resources.model.BlockStateModelLoader;
import net.minecraft.client.resources.model.ModelBakery;
import net.minecraft.client.resources.model.ModelDiscovery;
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.client.resources.model.ResolvedModel;
import net.minecraft.client.resources.model.UnbakedModel;
import net.minecraft.client.resources.model.cuboid.ItemModelGenerator;
import net.minecraft.client.resources.model.cuboid.MissingCuboidModel;
import net.minecraft.client.resources.model.sprite.MaterialBaker;
import net.minecraft.data.AtlasIds;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.world.level.block.state.BlockState;

/**
 * Vanilla's block models baked by its own loaders and {@link ModelBakery}, on sprites that each own a
 * {@code [2c, 2c+1] x [2r, 2r+1]} cell of a virtual atlas: atlas UVs carry the sprite and its local UV.
 */
public final class VanillaModels {
    private static final int GRID = 64;

    public final BlockStateModelSet blocks;
    public final FluidStateModelSet fluids;
    private final List<TextureAtlasSprite> byCell = new ArrayList<>();

    private static final class CellSprite extends TextureAtlasSprite {
        CellSprite(Identifier atlas, SpriteContents contents, int cell) {
            super(atlas, contents, contents.width(), contents.height(),
                    2 * (cell % GRID) * contents.width(), 2 * (cell / GRID) * contents.height(), 0);
        }
    }

    private SpriteLoader.Preparations atlas(ResourceManager resources, Identifier atlasId, Identifier location) {
        return atlas(resources, atlasId, location, byCell);
    }

    /** One atlas whose sprites take the next cells of {@code byCell}. */
    public static SpriteLoader.Preparations atlas(
            ResourceManager resources, Identifier atlasId, Identifier location, List<TextureAtlasSprite> byCell) {
        SpriteResourceLoader loader = SpriteResourceLoader.create(Set.of());
        Map<Identifier, TextureAtlasSprite> regions = new HashMap<>();
        TextureAtlasSprite missing = null;
        for (SpriteSource.Loader source : SpriteSourceList.load(resources, atlasId).list(resources)) {
            SpriteContents contents = source.get(loader);
            if (contents == null) {
                continue;
            }
            TextureAtlasSprite sprite = new CellSprite(location, contents, byCell.size());
            byCell.add(sprite);
            regions.put(contents.name(), sprite);
            if (missing == null) {
                missing = sprite;
            }
        }
        return new SpriteLoader.Preparations(1, 1, 0, missing, regions, CompletableFuture.completedFuture(null));
    }

    /** The sprite whose cell holds atlas UV {@code (u, v)}. */
    public TextureAtlasSprite spriteAt(float u, float v) {
        int col = (int) Math.floor(u / 2.0F);
        int row = (int) Math.floor(v / 2.0F);
        int cell = row * GRID + col;
        return cell >= 0 && cell < byCell.size() && col >= 0 && col < GRID ? byCell.get(cell) : null;
    }

    @SuppressWarnings({"unchecked", "deprecation"})
    public VanillaModels(ResourceManager resources) {
        Executor direct = Runnable::run;
        SpriteLoader.Preparations blockAtlas = atlas(resources, AtlasIds.BLOCKS, TextureAtlas.LOCATION_BLOCKS);
        SpriteLoader.Preparations itemAtlas = atlas(resources, AtlasIds.ITEMS, TextureAtlas.LOCATION_ITEMS);
        MaterialBaker materials = new MaterialBaker(blockAtlas, itemAtlas);
        Map<Identifier, UnbakedModel> unbaked = unbakedModels(resources);
        BlockStateModelLoader.LoadedModels states = BlockStateModelLoader.loadBlockStates(resources, direct).join();
        ModelDiscovery discovery = new ModelDiscovery(unbaked, MissingCuboidModel.missingModel());
        discovery.addSpecialModel(ItemModelGenerator.GENERATED_ITEM_MODEL_ID, new ItemModelGenerator());
        states.models().values().forEach(discovery::addRoot);
        Map<Identifier, ResolvedModel> resolved = discovery.resolve();
        ModelBakery bakery = new ModelBakery(EntityModelSet.EMPTY, null, null, states.models(), Map.of(), resolved,
                discovery.missingModel());
        ModelBakery.BakingResult baked = bakery.bakeModels(materials, direct).join();
        Map<BlockState, BlockStateModel> models = baked.blockStateModels();
        this.blocks = new BlockStateModelSet(models, baked.missingModels().block());
        this.fluids = new FluidStateModelSet(FluidStateModelSet.bake(materials), baked.missingModels().fluid());
    }

    /** Every {@code models/*.json} of the pack, through {@code ModelManager.loadBlockModels}. */
    @SuppressWarnings("unchecked")
    public static Map<Identifier, UnbakedModel> unbakedModels(ResourceManager resources) {
        Executor direct = Runnable::run;
        try {
            Method load = ModelManager.class.getDeclaredMethod("loadBlockModels", ResourceManager.class, Executor.class);
            load.setAccessible(true);
            return ((CompletableFuture<Map<Identifier, UnbakedModel>>) load.invoke(null, resources, direct)).join();
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("loading block models", e);
        }
    }

    public boolean hasModel(BlockState state) {
        return blocks.get(state) != blocks.missingModel();
    }
}
