package dev.mcr.oracle.mesh;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import dev.mcr.oracle.model.BlockModelHarness;
import dev.mcr.oracle.model.VanillaModels;
import java.util.ArrayList;
import java.util.EnumMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import net.minecraft.client.color.block.BlockColors;
import net.minecraft.client.renderer.block.BlockModelLighter;
import net.minecraft.client.renderer.block.BlockQuadOutput;
import net.minecraft.client.renderer.block.FluidRenderer;
import net.minecraft.client.renderer.block.ModelBlockRenderer;
import net.minecraft.client.renderer.chunk.ChunkSectionLayer;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;

/**
 * A generated section meshed by {@link ModelBlockRenderer} and {@link FluidRenderer} the way {@code SectionCompiler}
 * drives them (AO on, cutout leaves on); see mcr-parity's section_mesh.rs for the normal form.
 */
public final class SectionMeshHarness implements Harness {
    private static final double STEP = 1.0 / 1024.0;

    private final VanillaModels models;
    private final BlockColors colors = BlockColors.createDefault();

    public SectionMeshHarness(Boot boot) {
        Boot.tags();
        this.models = new VanillaModels(boot.clientResources());
    }

    private static String name(ChunkSectionLayer layer) {
        return layer.name().toLowerCase(Locale.ROOT);
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        JsonArray section = testCase.getAsJsonArray("section");
        BlockPos origin = new BlockPos(section.get(0).getAsInt() * 16, section.get(1).getAsInt() * 16,
                section.get(2).getAsInt() * 16);
        List<BlockState> palette = new ArrayList<>();
        for (JsonElement entry : testCase.getAsJsonArray("palette")) {
            palette.add(BlockModelHarness.state(entry.getAsJsonObject()));
        }
        JsonArray cells = testCase.getAsJsonArray("cells");
        JsonArray lights = testCase.getAsJsonArray("light");
        BlockState[] states = new BlockState[cells.size()];
        int[] light = new int[cells.size()];
        for (int i = 0; i < states.length; i++) {
            states[i] = palette.get(cells.get(i).getAsInt());
            light[i] = lights.get(i).getAsInt();
        }
        JsonObject tint = testCase.getAsJsonObject("tint");
        CaseLevel level = new CaseLevel(origin, states, light, tint.get("grass").getAsInt(), tint.get("foliage").getAsInt(),
                tint.get("dry_foliage").getAsInt(), tint.get("water").getAsInt());

        Map<ChunkSectionLayer, QuadRecorder> layers = new EnumMap<>(ChunkSectionLayer.class);
        for (ChunkSectionLayer layer : ChunkSectionLayer.values()) {
            layers.put(layer, new QuadRecorder(name(layer)));
        }
        BlockQuadOutput output = (x, y, z, quad, instance) -> layers.get(quad.materialInfo().layer()).putBlockBakedQuad(x, y, z, quad, instance);
        BlockQuadOutput opaque = (x, y, z, quad, instance) -> layers.get(ChunkSectionLayer.SOLID).putBlockBakedQuad(x, y, z, quad, instance);
        ModelBlockRenderer blocks = new ModelBlockRenderer(true, true, colors);
        FluidRenderer fluids = new FluidRenderer(models.fluids);
        BlockModelLighter.enableCaching();
        try {
            for (BlockPos pos : BlockPos.betweenClosed(origin, origin.offset(15, 15, 15))) {
                BlockState state = level.getBlockState(pos);
                if (state.isAir()) {
                    continue;
                }
                FluidState fluid = state.getFluidState();
                if (!fluid.isEmpty()) {
                    fluids.tesselate(level, pos, layers::get, state, fluid);
                }
                if (state.getRenderShape() == RenderShape.MODEL) {
                    blocks.tesselateBlock(ModelBlockRenderer.forceOpaque(true, state) ? opaque : output,
                            pos.getX() & 15, pos.getY() & 15, pos.getZ() & 15, level, pos, state, models.blocks.get(state),
                            state.getSeed(pos));
                }
            }
        } finally {
            BlockModelLighter.clearCache();
        }
        return normalize(layers.values());
    }

    /** Mirrors section_mesh.rs's {@code normalize}: per layer, quads by centre, sorted. */
    private JsonObject normalize(Iterable<QuadRecorder> recorders) {
        Map<String, Map<String, List<Object[]>>> layers = new TreeMap<>();
        for (QuadRecorder recorder : recorders) {
            for (int q = 0; q + 3 < recorder.vertices.size(); q += 4) {
                double[][] v = new double[4][];
                double u = 0;
                double w = 0;
                for (int i = 0; i < 4; i++) {
                    v[i] = recorder.vertices.get(q + i).clone();
                    u += v[i][3] / 4;
                    w += v[i][4] / 4;
                }
                TextureAtlasSprite sprite = models.spriteAt((float) u, (float) w);
                String spriteName = sprite == null ? "?" : sprite.contents().name().toString();
                for (double[] x : v) {
                    if (sprite != null) {
                        x[3] = (x[3] - sprite.getU0()) / (sprite.getU1() - sprite.getU0());
                        x[4] = (x[4] - sprite.getV0()) / (sprite.getV1() - sprite.getV0());
                    }
                }
                for (int a = 3; a < 5; a++) {
                    double min = Double.POSITIVE_INFINITY;
                    for (double[] x : v) {
                        min = Math.min(min, x[a]);
                    }
                    double shift = Math.floor(min + 1.0 / 2048.0);
                    for (double[] x : v) {
                        x[a] -= shift;
                    }
                }
                for (double[] x : v) {
                    for (int c = 0; c < 5; c++) {
                        x[c] = Harness.round(x[c], STEP);
                    }
                }
                int start = compare(v[2], v[0]) < 0 ? 2 : 0;
                double[][] r = new double[4][];
                for (int i = 0; i < 4; i++) {
                    r[i] = v[(start + i) % 4];
                }
                long[] centre = new long[3];
                for (int a = 0; a < 3; a++) {
                    double sum = 0;
                    for (double[] x : r) {
                        sum += x[a];
                    }
                    centre[a] = (long) Math.floor(sum * 2.0 + 0.5625);
                }
                String key = centre[0] + "," + centre[1] + "," + centre[2];
                long[] facing = facing(r);
                StringBuilder sort = new StringBuilder(spriteName).append(';').append(facing[0]).append(',').append(facing[1])
                        .append(',').append(facing[2]).append(';');
                JsonArray positions = new JsonArray();
                JsonArray colors = new JsonArray();
                JsonArray lights = new JsonArray();
                boolean first = true;
                for (double[] x : r) {
                    JsonArray p = new JsonArray();
                    for (int c = 0; c < 5; c++) {
                        if (!first) {
                            sort.append(',');
                        }
                        first = false;
                        sort.append((long) (x[c] * 1024.0));
                        p.add(x[c]);
                    }
                    JsonArray color = new JsonArray();
                    for (int c = 5; c < 8; c++) {
                        color.add((int) x[c]);
                    }
                    JsonArray l = new JsonArray();
                    l.add((int) x[8]);
                    l.add((int) x[9]);
                    positions.add(p);
                    colors.add(color);
                    lights.add(l);
                }
                JsonObject quad = new JsonObject();
                quad.addProperty("sprite", spriteName);
                JsonArray normal = new JsonArray();
                for (long c : facing) {
                    normal.add(c);
                }
                quad.add("n", normal);
                quad.add("p", positions);
                quad.add("c", colors);
                quad.add("l", lights);
                layers.computeIfAbsent(recorder.layer, k -> new TreeMap<>()).computeIfAbsent(key, k -> new ArrayList<>())
                        .add(new Object[]{sort.toString(), quad});
            }
        }
        JsonObject out = new JsonObject();
        layers.forEach((layer, byKey) -> {
            JsonObject m = new JsonObject();
            byKey.forEach((key, list) -> {
                list.sort((a, b) -> ((String) a[0]).compareTo((String) b[0]));
                JsonArray array = new JsonArray();
                list.forEach(e -> array.add((JsonObject) e[1]));
                m.add(key, array);
            });
            out.add(layer, m);
        });
        return out;
    }

    private static int compare(double[] a, double[] b) {
        for (int i = 0; i < a.length; i++) {
            int c = Double.compare(a[i], b[i]);
            if (c != 0) {
                return c;
            }
        }
        return 0;
    }

    private static long[] facing(double[][] p) {
        double[] a = {p[2][0] - p[1][0], p[2][1] - p[1][1], p[2][2] - p[1][2]};
        double[] b = {p[0][0] - p[1][0], p[0][1] - p[1][1], p[0][2] - p[1][2]};
        double[] n = {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
        double len = Math.sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
        long[] out = new long[3];
        if (len != 0.0) {
            for (int i = 0; i < 3; i++) {
                out[i] = (long) Math.floor(n[i] / len * 8.0 + 0.5);
            }
        }
        return out;
    }
}
