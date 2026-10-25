package dev.mcr.oracle.entity;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import java.lang.reflect.Field;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.model.EntityModel;
import net.minecraft.client.model.geom.EntityModelSet;
import net.minecraft.client.model.geom.ModelLayerLocation;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.renderer.entity.state.ArmedEntityRenderState;
import net.minecraft.client.renderer.entity.state.EntityRenderState;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.component.SwingAnimation;

/** {@code EntityModel.setupAnim} on a baked layer and a generated render state; see mcr-parity's entity_pose.rs. */
public final class EntityPoseHarness implements Harness {
    private static final double STEP = 1.0 / 4096.0;

    private final EntityModelSet models = EntityModelSet.vanilla();
    private final Map<String, EntityModel<EntityRenderState>> cache = new HashMap<>();
    private final Map<String, ModelPart> roots = new HashMap<>();
    private final Field children;

    public EntityPoseHarness(Boot boot) {
        try {
            children = ModelPart.class.getDeclaredField("children");
            children.setAccessible(true);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("ModelPart.children", e);
        }
    }

    @SuppressWarnings("unchecked")
    private EntityModel<EntityRenderState> model(String layer, String type) throws ReflectiveOperationException {
        String key = layer + "|" + type;
        EntityModel<EntityRenderState> model = cache.get(key);
        if (model == null) {
            int hash = layer.indexOf('#');
            ModelPart root = models.bakeLayer(new ModelLayerLocation(Identifier.parse(layer.substring(0, hash)), layer.substring(hash + 1)));
            model = (EntityModel<EntityRenderState>) Class.forName("net.minecraft.client.model." + type)
                    .getConstructor(ModelPart.class)
                    .newInstance(root);
            cache.put(key, model);
            roots.put(key, root);
        }
        return model;
    }

    private static Field field(Class<?> type, String name) throws NoSuchFieldException {
        for (Class<?> c = type; c != null; c = c.getSuperclass()) {
            try {
                return c.getDeclaredField(name);
            } catch (NoSuchFieldException ignored) {
                // keep looking in the superclass
            }
        }
        throw new NoSuchFieldException(type.getName() + "." + name);
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        String layer = testCase.get("layer").getAsString();
        JsonObject out = new JsonObject();
        out.addProperty("layer", layer);
        try {
            EntityModel<EntityRenderState> model = model(layer, testCase.get("model").getAsString());
            EntityRenderState state = (EntityRenderState) Class.forName(
                            "net.minecraft.client.renderer.entity.state." + testCase.get("state").getAsString())
                    .getConstructor()
                    .newInstance();
            for (Map.Entry<String, JsonElement> e : testCase.getAsJsonObject("vanilla").entrySet()) {
                Field f = field(state.getClass(), e.getKey());
                f.setAccessible(true);
                Class<?> t = f.getType();
                if (t == float.class) {
                    f.setFloat(state, e.getValue().getAsFloat());
                } else if (t == int.class) {
                    f.setInt(state, e.getValue().getAsInt());
                } else if (t == boolean.class) {
                    f.setBoolean(state, e.getValue().getAsBoolean());
                } else {
                    throw new IllegalArgumentException("field type " + t + " for " + e.getKey());
                }
            }
            if (state instanceof ArmedEntityRenderState armed && armed.swingAnimation > 0.0F) {
                armed.currentSwing = new LivingEntity.SwingDescription(InteractionHand.MAIN_HAND, SwingAnimation.DEFAULT, SwingAnimation.DEFAULT.duration());
            }
            model.setupAnim(state);
            JsonObject parts = new JsonObject();
            // Some models keep a child as their root(); the baked layer is what ours poses.
            walk(roots.get(layer + "|" + testCase.get("model").getAsString()), "", parts);
            out.add("parts", parts);
        } catch (ReflectiveOperationException | RuntimeException e) {
            System.err.println("[entity_pose] " + layer + ": " + e);
            out.addProperty("error", e.getClass().getSimpleName());
        }
        return out;
    }

    @SuppressWarnings("unchecked")
    private void walk(ModelPart part, String path, JsonObject out) throws IllegalAccessException {
        JsonObject pose = new JsonObject();
        if (!part.visible || part.skipDraw) {
            pose.addProperty("hidden", true);
        } else {
            pose.add("pos", triple(part.x, part.y, part.z));
            pose.add("rot", triple(part.xRot, part.yRot, part.zRot));
            pose.add("scale", triple(part.xScale, part.yScale, part.zScale));
        }
        out.add(path, pose);
        for (Map.Entry<String, ModelPart> child : ((Map<String, ModelPart>) children.get(part)).entrySet()) {
            walk(child.getValue(), path.isEmpty() ? child.getKey() : path + "/" + child.getKey(), out);
        }
    }

    private static JsonArray triple(float a, float b, float c) {
        JsonArray out = new JsonArray();
        out.add(Harness.round(a, STEP));
        out.add(Harness.round(b, STEP));
        out.add(Harness.round(c, STEP));
        return out;
    }
}
