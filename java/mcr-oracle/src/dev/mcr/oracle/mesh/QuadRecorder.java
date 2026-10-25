package dev.mcr.oracle.mesh;

import com.mojang.blaze3d.vertex.VertexConsumer;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.util.ARGB;

/** Keeps every vertex a renderer writes, as {@code [x, y, z, u, v, r, g, b, block, sky]}, four to a quad. */
final class QuadRecorder implements VertexConsumer {
    final String layer;
    final List<double[]> vertices = new ArrayList<>();
    private double[] current;

    QuadRecorder(String layer) {
        this.layer = layer;
    }

    @Override
    public VertexConsumer addVertex(float x, float y, float z) {
        current = new double[10];
        current[0] = x;
        current[1] = y;
        current[2] = z;
        vertices.add(current);
        return this;
    }

    @Override
    public VertexConsumer setColor(int r, int g, int b, int a) {
        current[5] = r;
        current[6] = g;
        current[7] = b;
        return this;
    }

    @Override
    public VertexConsumer setColor(int color) {
        return setColor(ARGB.red(color), ARGB.green(color), ARGB.blue(color), ARGB.alpha(color));
    }

    @Override
    public VertexConsumer setUv(float u, float v) {
        current[3] = u;
        current[4] = v;
        return this;
    }

    @Override
    public VertexConsumer setUv1(int u, int v) {
        return this;
    }

    @Override
    public VertexConsumer setUv2(int u, int v) {
        current[8] = u;
        current[9] = v;
        return this;
    }

    @Override
    public VertexConsumer setUv3(float u, float v) {
        return this;
    }

    @Override
    public VertexConsumer setNormal(float x, float y, float z) {
        return this;
    }

    @Override
    public VertexConsumer setLineWidth(float width) {
        return this;
    }
}
