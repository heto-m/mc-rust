package dev.mcr.testmod;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import net.fabricmc.fabric.api.client.rendering.v1.LivingEntityRenderLayerRegistrationCallback;
import net.minecraft.client.model.EntityModel;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.RenderLayerParent;
import net.minecraft.client.renderer.entity.layers.RenderLayer;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.world.entity.EntityTypes;

/** A solid magenta cube over every armor stand's head: a mod-added render layer the bridge tests probe for. */
final class MarkerLayer<S extends EntityRenderState, M extends EntityModel<? super S>> extends RenderLayer<S, M> {
    static final int COLOR = 0xFFFF00FF;
    private static final float HALF = 0.2f;
    /** Layer space is the model's: y points down and 0 sits about 1.5 blocks above the feet. */
    private static final float CENTER_Y = -0.75f;

    private MarkerLayer(RenderLayerParent<S, M> parent) {
        super(parent);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    static void register() {
        LivingEntityRenderLayerRegistrationCallback.EVENT.register((type, renderer, helper, context) -> {
            if (type == EntityTypes.ARMOR_STAND) {
                helper.register(new MarkerLayer((RenderLayerParent) renderer));
            }
        });
    }

    @Override
    public void submit(PoseStack poseStack, SubmitNodeCollector collector, int lightCoords, S state, float yRot, float xRot) {
        collector.submitCustomGeometry(poseStack, RenderTypes.debugQuads(), MarkerLayer::cube);
    }

    private static void cube(PoseStack.Pose pose, VertexConsumer buffer) {
        float a = -HALF;
        float b = HALF;
        float y0 = CENTER_Y - HALF;
        float y1 = CENTER_Y + HALF;
        quad(pose, buffer, a, y0, a, b, y0, a, b, y1, a, a, y1, a);
        quad(pose, buffer, a, y0, b, a, y1, b, b, y1, b, b, y0, b);
        quad(pose, buffer, a, y0, a, a, y1, a, a, y1, b, a, y0, b);
        quad(pose, buffer, b, y0, a, b, y0, b, b, y1, b, b, y1, a);
        quad(pose, buffer, a, y0, a, a, y0, b, b, y0, b, b, y0, a);
        quad(pose, buffer, a, y1, a, b, y1, a, b, y1, b, a, y1, b);
    }

    private static void quad(
            PoseStack.Pose pose, VertexConsumer buffer,
            float x0, float y0, float z0, float x1, float y1, float z1,
            float x2, float y2, float z2, float x3, float y3, float z3) {
        buffer.addVertex(pose, x0, y0, z0).setColor(COLOR);
        buffer.addVertex(pose, x1, y1, z1).setColor(COLOR);
        buffer.addVertex(pose, x2, y2, z2).setColor(COLOR);
        buffer.addVertex(pose, x3, y3, z3).setColor(COLOR);
    }
}
