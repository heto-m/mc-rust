import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import net.minecraft.SharedConstants;
import net.minecraft.client.animation.AnimationChannel;
import net.minecraft.client.animation.AnimationDefinition;
import net.minecraft.client.animation.Keyframe;
import net.minecraft.client.model.geom.LayerDefinitions;
import net.minecraft.client.model.geom.ModelLayerLocation;
import net.minecraft.client.model.geom.PartPose;
import net.minecraft.client.model.geom.builders.CubeDefinition;
import net.minecraft.client.model.geom.builders.CubeDeformation;
import net.minecraft.client.model.geom.builders.LayerDefinition;
import net.minecraft.client.model.geom.builders.MaterialDefinition;
import net.minecraft.client.model.geom.builders.MeshDefinition;
import net.minecraft.client.model.geom.builders.PartDefinition;
import net.minecraft.client.model.geom.builders.UVPair;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import org.joml.Vector3fc;

public final class EntityModelExtractor {
    private EntityModelExtractor() {}

    public static void main(String[] args) throws Exception {
        Path out = Path.of(args[0]);
        Files.createDirectories(out);
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        write(out.resolve("entity_models.json"), layers());
        write(out.resolve("animations.json"), animations());
        write(out.resolve("model_layer_fields.json"), layerFields());
        System.exit(0);
    }

    private static void write(Path path, JsonElement json) throws Exception {
        Files.writeString(path, new GsonBuilder().create().toJson(json), StandardCharsets.UTF_8);
    }

    private static Object get(Object o, String name) throws Exception {
        Field f = o.getClass().getDeclaredField(name);
        f.setAccessible(true);
        return f.get(o);
    }

    private static JsonObject layers() throws Exception {
        Map<String, JsonObject> sorted = new TreeMap<>();
        for (Map.Entry<ModelLayerLocation, LayerDefinition> e : LayerDefinitions.createRoots().entrySet()) {
            LayerDefinition def = e.getValue();
            MeshDefinition mesh = (MeshDefinition) get(def, "mesh");
            MaterialDefinition mat = (MaterialDefinition) get(def, "material");
            JsonObject layer = new JsonObject();
            layer.addProperty("tex_w", (int) get(mat, "xTexSize"));
            layer.addProperty("tex_h", (int) get(mat, "yTexSize"));
            layer.add("root", part(mesh.getRoot()));
            sorted.put(e.getKey().toString(), layer);
        }
        JsonObject root = new JsonObject();
        sorted.forEach(root::add);
        return root;
    }

    @SuppressWarnings("unchecked")
    private static JsonObject part(PartDefinition p) throws Exception {
        JsonObject o = new JsonObject();
        PartPose pose = (PartPose) get(p, "partPose");
        o.add("pose", array(pose.x(), pose.y(), pose.z(), pose.xRot(), pose.yRot(), pose.zRot(), pose.xScale(), pose.yScale(), pose.zScale()));
        JsonArray cubes = new JsonArray();
        for (CubeDefinition c : (List<CubeDefinition>) get(p, "cubes")) {
            cubes.add(cube(c));
        }
        o.add("cubes", cubes);
        JsonObject children = new JsonObject();
        Map<String, PartDefinition> sorted = new TreeMap<>();
        p.getChildren().forEach(e -> sorted.put(e.getKey(), e.getValue()));
        for (Map.Entry<String, PartDefinition> e : sorted.entrySet()) {
            children.add(e.getKey(), part(e.getValue()));
        }
        o.add("children", children);
        return o;
    }

    @SuppressWarnings("unchecked")
    private static JsonObject cube(CubeDefinition c) throws Exception {
        JsonObject o = new JsonObject();
        Vector3fc origin = (Vector3fc) get(c, "origin");
        Vector3fc dim = (Vector3fc) get(c, "dimensions");
        CubeDeformation grow = (CubeDeformation) get(c, "grow");
        UVPair uv = (UVPair) get(c, "texCoord");
        UVPair scale = (UVPair) get(c, "texScale");
        o.add("origin", array(origin.x(), origin.y(), origin.z()));
        o.add("size", array(dim.x(), dim.y(), dim.z()));
        o.add("uv", array(uv.u(), uv.v()));
        o.add("grow", array((float) get(grow, "growX"), (float) get(grow, "growY"), (float) get(grow, "growZ")));
        o.addProperty("mirror", (boolean) get(c, "mirror"));
        if (scale.u() != 1.0F || scale.v() != 1.0F) {
            o.add("tex_scale", array(scale.u(), scale.v()));
        }
        List<String> names = new ArrayList<>();
        for (Direction d : (Set<Direction>) get(c, "visibleFaces")) {
            names.add(d.getName());
        }
        if (names.size() != 6) {
            JsonArray faces = new JsonArray();
            names.stream().sorted().forEach(faces::add);
            o.add("faces", faces);
        }
        return o;
    }

    private static JsonArray array(float... v) {
        JsonArray a = new JsonArray();
        for (float f : v) {
            a.add(f);
        }
        return a;
    }

    private static JsonObject layerFields() throws Exception {
        Map<String, String> sorted = new TreeMap<>();
        for (Field f : net.minecraft.client.model.geom.ModelLayers.class.getDeclaredFields()) {
            if (Modifier.isStatic(f.getModifiers()) && f.getType() == ModelLayerLocation.class) {
                f.setAccessible(true);
                sorted.put(f.getName(), f.get(null).toString());
            }
        }
        JsonObject root = new JsonObject();
        sorted.forEach(root::addProperty);
        return root;
    }

    private static JsonObject animations() throws Exception {
        Map<String, JsonObject> sorted = new TreeMap<>();
        for (Class<?> cls : classesIn("net.minecraft.client.animation.definitions")) {
            for (Field f : cls.getDeclaredFields()) {
                if (Modifier.isStatic(f.getModifiers()) && f.getType() == AnimationDefinition.class) {
                    f.setAccessible(true);
                    sorted.put(cls.getSimpleName() + "." + f.getName(), animation((AnimationDefinition) f.get(null)));
                }
            }
        }
        JsonObject root = new JsonObject();
        sorted.forEach(root::add);
        return root;
    }

    private static List<Class<?>> classesIn(String pkg) throws Exception {
        String dir = pkg.replace('.', '/');
        String path = EntityModelExtractor.class.getClassLoader().getResource(dir).getPath();
        String jar = path.substring(5, path.indexOf('!'));
        List<Class<?>> out = new ArrayList<>();
        try (ZipFile zf = new ZipFile(jar)) {
            List<String> names = new ArrayList<>();
            zf.stream().map(ZipEntry::getName)
                .filter(n -> n.startsWith(dir + "/") && n.endsWith(".class") && !n.contains("$") && !n.contains("package-info"))
                .forEach(names::add);
            names.sort(null);
            for (String n : names) {
                out.add(Class.forName(n.substring(0, n.length() - 6).replace('/', '.')));
            }
        }
        return out;
    }

    private static JsonObject animation(AnimationDefinition def) {
        JsonObject o = new JsonObject();
        o.addProperty("length", def.lengthInSeconds());
        o.addProperty("looping", def.looping());
        JsonObject bones = new JsonObject();
        new TreeMap<>(def.boneAnimations()).forEach((bone, channels) -> {
            JsonArray arr = new JsonArray();
            for (AnimationChannel ch : channels) {
                JsonObject c = new JsonObject();
                String target = ch.target() == AnimationChannel.Targets.POSITION ? "position"
                    : ch.target() == AnimationChannel.Targets.ROTATION ? "rotation" : "scale";
                c.addProperty("target", target);
                JsonArray kfs = new JsonArray();
                for (Keyframe k : ch.keyframes()) {
                    JsonObject kf = new JsonObject();
                    kf.addProperty("t", k.timestamp());
                    kf.add("pre", array(k.preTarget().x(), k.preTarget().y(), k.preTarget().z()));
                    kf.add("post", array(k.postTarget().x(), k.postTarget().y(), k.postTarget().z()));
                    kf.addProperty("interp", k.interpolation() == AnimationChannel.Interpolations.CATMULLROM ? "catmullrom" : "linear");
                    kfs.add(kf);
                }
                c.add("keyframes", kfs);
                arr.add(c);
            }
            bones.add(bone, arr);
        });
        o.add("bones", bones);
        return o;
    }
}
