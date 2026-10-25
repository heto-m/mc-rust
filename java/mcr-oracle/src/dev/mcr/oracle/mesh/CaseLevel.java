package dev.mcr.oracle.mesh;

import net.minecraft.client.renderer.BiomeColors;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.CardinalLighting;
import net.minecraft.world.level.ColorResolver;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.lighting.LevelLightEngine;
import net.minecraft.world.level.material.FluidState;

/** One case's section and its one-block border; air and darkness beyond it, one biome tint per resolver. */
final class CaseLevel implements BlockAndTintGetter {
    static final int PAD = 18;

    private final BlockPos origin;
    private final BlockState[] states;
    private final int[] light;
    private final int grass;
    private final int foliage;
    private final int dryFoliage;
    private final int water;

    CaseLevel(BlockPos origin, BlockState[] states, int[] light, int grass, int foliage, int dryFoliage, int water) {
        this.origin = origin;
        this.states = states;
        this.light = light;
        this.grass = grass;
        this.foliage = foliage;
        this.dryFoliage = dryFoliage;
        this.water = water;
    }

    static int index(int x, int y, int z) {
        return ((y + 1) * PAD + (z + 1)) * PAD + (x + 1);
    }

    private int at(BlockPos pos) {
        int x = pos.getX() - origin.getX();
        int y = pos.getY() - origin.getY();
        int z = pos.getZ() - origin.getZ();
        if (x < -1 || y < -1 || z < -1 || x > 16 || y > 16 || z > 16) {
            return -1;
        }
        return index(x, y, z);
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        int i = at(pos);
        return i < 0 ? Blocks.AIR.defaultBlockState() : states[i];
    }

    @Override
    public FluidState getFluidState(BlockPos pos) {
        return getBlockState(pos).getFluidState();
    }

    @Override
    public BlockEntity getBlockEntity(BlockPos pos) {
        return null;
    }

    @Override
    public int getBrightness(LightLayer layer, BlockPos pos) {
        int i = at(pos);
        if (i < 0) {
            return 0;
        }
        return layer == LightLayer.SKY ? light[i] >> 4 : light[i] & 15;
    }

    @Override
    public LevelLightEngine getLightEngine() {
        return LevelLightEngine.EMPTY;
    }

    @Override
    public CardinalLighting cardinalLighting() {
        return CardinalLighting.DEFAULT;
    }

    @Override
    public int getBlockTint(BlockPos pos, ColorResolver resolver) {
        int rgb;
        if (resolver == BiomeColors.GRASS_COLOR_RESOLVER) {
            rgb = grass;
        } else if (resolver == BiomeColors.FOLIAGE_COLOR_RESOLVER) {
            rgb = foliage;
        } else if (resolver == BiomeColors.DRY_FOLIAGE_COLOR_RESOLVER) {
            rgb = dryFoliage;
        } else if (resolver == BiomeColors.WATER_COLOR_RESOLVER) {
            rgb = water;
        } else {
            throw new IllegalArgumentException("unknown colour resolver " + resolver);
        }
        return 0xFF000000 | rgb;
    }

    @Override
    public int getHeight() {
        return 4096;
    }

    @Override
    public int getMinY() {
        return -2048;
    }
}
