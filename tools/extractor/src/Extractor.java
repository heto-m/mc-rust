import java.io.IOException;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SupportType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FlowingFluid;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

public final class Extractor {
    private Extractor() {}

    public static void main(String[] args) throws IOException {
        Path out = Path.of(args[0]);
        Files.createDirectories(out);
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var lookup = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(DataComponentInitializers.PendingComponents::apply);
        writeBlocks(out.resolve("blocks.json"));
        writeItems(out.resolve("items.json"));
        writeEntities(out.resolve("entity_types.json"));
        writeFluids(out.resolve("fluids.json"));
        System.exit(0);
    }

    private static PrintWriter open(Path p) throws IOException {
        return new PrintWriter(Files.newBufferedWriter(p, StandardCharsets.UTF_8));
    }

    private static String str(String s) {
        return "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    private static String shape(VoxelShape shape) {
        List<String> boxes = new ArrayList<>();
        for (AABB b : shape.toAabbs()) {
            boxes.add("[" + b.minX + "," + b.minY + "," + b.minZ + "," + b.maxX + "," + b.maxY + "," + b.maxZ + "]");
        }
        return "[" + String.join(",", boxes) + "]";
    }

    private static void writeBlocks(Path path) throws IOException {
        try (PrintWriter w = open(path)) {
            w.println("{\"blocks\":[");
            boolean firstBlock = true;
            for (Block block : BuiltInRegistries.BLOCK) {
                BlockState def = block.defaultBlockState();
                if (!firstBlock) w.println(",");
                firstBlock = false;
                w.print("{\"name\":" + str(BuiltInRegistries.BLOCK.getKey(block).toString()));
                w.print(",\"friction\":" + block.getFriction());
                w.print(",\"speed_factor\":" + block.getSpeedFactor());
                w.print(",\"jump_factor\":" + block.getJumpFactor());
                w.print(",\"destroy_time\":" + block.defaultDestroyTime());
                w.print(",\"requires_tool\":" + def.requiresCorrectToolForDrops());
                w.print(",\"explosion_resistance\":" + block.getExplosionResistance());
                w.print(",\"sound_type\":" + str(def.getSoundType().getBreakSound().location().toString()));
                w.print(",\"states\":[");
                boolean firstState = true;
                for (BlockState s : block.getStateDefinition().getPossibleStates()) {
                    if (!firstState) w.print(",");
                    firstState = false;
                    w.print(state(s));
                }
                w.print("]}");
            }
            w.println("\n]}");
        }
    }

    private static String state(BlockState s) {
        BlockPos z = BlockPos.ZERO;
        var g = EmptyBlockGetter.INSTANCE;
        int sturdy = 0;
        int occlusionFaces = 0;
        for (Direction d : Direction.values()) {
            if (s.isFaceSturdy(g, z, d, SupportType.FULL)) sturdy |= 1 << d.ordinal();
            if (s.getFaceOcclusionShape(d).equals(Shapes.block())) occlusionFaces |= 1 << d.ordinal();
        }
        FluidState f = s.getFluidState();
        String fluid = "null";
        if (!f.isEmpty()) {
            boolean falling = f.hasProperty(FlowingFluid.FALLING) && f.getValue(FlowingFluid.FALLING);
            fluid = "{\"type\":" + str(BuiltInRegistries.FLUID.getKey(f.getType()).toString())
                + ",\"amount\":" + f.getAmount()
                + ",\"source\":" + f.isSource()
                + ",\"falling\":" + falling + "}";
        }
        StringBuilder b = new StringBuilder();
        b.append("{\"id\":").append(Block.getId(s));
        b.append(",\"collision\":").append(shape(s.getCollisionShape(g, z)));
        b.append(",\"outline\":").append(shape(s.getShape(g, z)));
        b.append(",\"occlusion\":").append(shape(s.getOcclusionShape()));
        b.append(",\"light_emission\":").append(s.getLightEmission());
        b.append(",\"light_block\":").append(s.getLightDampening());
        b.append(",\"propagates_skylight_down\":").append(s.propagatesSkylightDown());
        b.append(",\"can_occlude\":").append(s.canOcclude());
        b.append(",\"use_shape_for_light_occlusion\":").append(s.useShapeForLightOcclusion());
        b.append(",\"solid_render\":").append(s.isSolidRender());
        b.append(",\"full_collision\":").append(s.isCollisionShapeFullBlock(g, z));
        b.append(",\"sturdy_faces\":").append(sturdy);
        b.append(",\"occlusion_faces\":").append(occlusionFaces);
        b.append(",\"air\":").append(s.isAir());
        b.append(",\"liquid\":").append(s.liquid());
        b.append(",\"replaceable\":").append(s.canBeReplaced());
        b.append(",\"has_block_entity\":").append(s.hasBlockEntity());
        b.append(",\"redstone_conductor\":").append(s.isRedstoneConductor(g, z));
        b.append(",\"legacy_solid\":").append(s.isSolid());
        b.append(",\"map_color\":").append(s.getMapColor(g, z).id);
        b.append(",\"fluid\":").append(fluid);
        b.append("}");
        return b.toString();
    }

    private static void writeItems(Path path) throws IOException {
        try (PrintWriter w = open(path)) {
            w.println("[");
            boolean first = true;
            for (Item item : BuiltInRegistries.ITEM) {
                if (!first) w.println(",");
                first = false;
                Identifier key = BuiltInRegistries.ITEM.getKey(item);
                String block = "null";
                if (item instanceof BlockItem bi) {
                    block = str(BuiltInRegistries.BLOCK.getKey(bi.getBlock()).toString());
                }
                w.print("{\"id\":" + BuiltInRegistries.ITEM.getId(item) + ",\"name\":" + str(key.toString())
                    + ",\"max_stack_size\":" + item.getDefaultMaxStackSize()
                    + ",\"max_damage\":" + item.getDefaultInstance().getMaxDamage()
                    + ",\"block\":" + block + "}");
            }
            w.println("\n]");
        }
    }

    private static void writeEntities(Path path) throws IOException {
        try (PrintWriter w = open(path)) {
            w.println("[");
            boolean first = true;
            for (EntityType<?> t : BuiltInRegistries.ENTITY_TYPE) {
                if (!first) w.println(",");
                first = false;
                var d = t.getDimensions();
                w.print("{\"id\":" + BuiltInRegistries.ENTITY_TYPE.getId(t)
                    + ",\"name\":" + str(BuiltInRegistries.ENTITY_TYPE.getKey(t).toString())
                    + ",\"width\":" + d.width() + ",\"height\":" + d.height() + ",\"eye_height\":" + d.eyeHeight()
                    + ",\"category\":" + str(t.getCategory().getName()) + "}");
            }
            w.println("\n]");
        }
    }

    private static void writeFluids(Path path) throws IOException {
        try (PrintWriter w = open(path)) {
            w.println("[");
            boolean first = true;
            for (Fluid f : BuiltInRegistries.FLUID) {
                if (!first) w.println(",");
                first = false;
                w.print("{\"id\":" + BuiltInRegistries.FLUID.getId(f)
                    + ",\"name\":" + str(BuiltInRegistries.FLUID.getKey(f).toString())
                    + ",\"source\":" + f.isSource(f.defaultFluidState())
                    + ",\"empty\":" + f.defaultFluidState().isEmpty() + "}");
            }
            w.println("\n]");
        }
    }
}
