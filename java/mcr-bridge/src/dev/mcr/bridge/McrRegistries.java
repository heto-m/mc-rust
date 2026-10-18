package dev.mcr.bridge;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.component.TypedDataComponent;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.SupportType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.FlowingFluid;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Java's static registries for Rust: every registry's names, and for every block, item, entity type and fluid
 * the data mcr's extractor writes for vanilla. Rust rebuilds its tables from them, so it decodes chunks and
 * items exactly as Java does after Fabric's or NeoForge's registry sync.
 */
public final class McrRegistries {
    private static final String[] EQUIP_SLOTS = {"head", "chest", "legs", "feet", "body", "saddle", "mainhand", "offhand"};
    private static final int MAX_BOXES = 256;
    private static boolean titleSent;
    private static int problemsLogged;

    private McrRegistries() {}

    /** Once, at the first title screen: the client's own registries, so Rust can prepare before a join. */
    public static void reportAtTitle() {
        if (!titleSent) {
            titleSent = true;
            report("title");
        }
    }

    /** At the end of every configuration, after the loader's registry sync. */
    public static void report(String reason) {
        long start = System.nanoTime();
        String json;
        try {
            json = build(reason);
        } catch (RuntimeException | LinkageError e) {
            Bridge.info("registry_report_failed", e);
            return;
        }
        byte[] bytes = json.getBytes(StandardCharsets.UTF_8);
        Bridge bridge = Bridge.get();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment segment = arena.allocate(Math.max(bytes.length, 1));
            MemorySegment.copy(bytes, 0, segment, ValueLayout.JAVA_BYTE, 0, bytes.length);
            bridge.downcall(BridgeAbi.Entry.REGISTRY_REPORT).invokeExact(segment.address(), bytes.length);
        } catch (RuntimeException | Error e) {
            throw e;
        } catch (Throwable t) {
            throw new IllegalStateException(t);
        }
        Bridge.info("registry_report", reason + " " + bytes.length + " bytes in " + (System.nanoTime() - start) / 1_000_000 + " ms");
    }

    static String build(String reason) {
        List<String> problems = new ArrayList<>();
        StringBuilder w = new StringBuilder(1 << 20);
        w.append("{\"format\":2,\"reason\":").append(str(reason));
        w.append(",\"state_count\":").append(Block.BLOCK_STATE_REGISTRY.size());
        w.append(",\"registries\":{");
        boolean first = true;
        for (Registry<?> registry : BuiltInRegistries.REGISTRY) {
            if (!first) w.append(',');
            first = false;
            w.append(str(registry.key().identifier().toString())).append(":[");
            for (int i = 0; i < registry.size(); i++) {
                if (i > 0) w.append(',');
                Object value = registry.byId(i);
                var key = value == null ? null : keyOf(registry, value);
                w.append(str(key == null ? "mcr:missing_" + i : key));
            }
            w.append(']');
        }
        w.append('}');
        w.append(",\"blocks\":[");
        Map<String, Integer> shapes = new HashMap<>();
        List<String> shapeList = new ArrayList<>();
        Map<String, Integer> fluids = new HashMap<>();
        List<String> fluidList = new ArrayList<>();
        writeBlocks(w, problems, shapes, shapeList, fluids, fluidList);
        w.append("],\"items\":[");
        writeItems(w, problems);
        w.append("],\"entity_types\":[");
        writeEntities(w);
        w.append("],\"fluids\":[");
        writeFluids(w);
        w.append("],\"shapes\":[").append(String.join(",", shapeList));
        w.append("],\"fluid_states\":[").append(String.join(",", fluidList)).append("]");
        unregisteredStates(problems);
        w.append(",\"problems\":[");
        for (int i = 0; i < Math.min(problems.size(), 64); i++) {
            if (i > 0) w.append(',');
            w.append(str(problems.get(i)));
        }
        w.append("]}");
        return w.toString();
    }

    /** States in the id map whose block is not in the block registry: they still widen the direct palette. */
    private static void unregisteredStates(List<String> problems) {
        Map<String, Integer> owners = new HashMap<>();
        int unregistered = 0;
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            Block block = state.getBlock();
            if (BuiltInRegistries.BLOCK.getValue(BuiltInRegistries.BLOCK.getKey(block)) != block) {
                unregistered++;
                owners.merge(block.getClass().getName(), 1, Integer::sum);
            }
        }
        if (unregistered > 0) {
            problem(problems, unregistered + " states of unregistered blocks in the state id map: " + owners);
        }
        int iterated = 0;
        int maxId = -1;
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            iterated++;
            maxId = Math.max(maxId, Block.BLOCK_STATE_REGISTRY.getId(state));
        }
        problem(problems, "state id map: size " + Block.BLOCK_STATE_REGISTRY.size() + ", iterated " + iterated + ", max id " + maxId
            + ", class " + Block.BLOCK_STATE_REGISTRY.getClass().getName());
    }

    @SuppressWarnings("unchecked")
    private static <T> String keyOf(Registry<T> registry, Object value) {
        var key = registry.getKey((T) value);
        return key == null ? null : key.toString();
    }

    private static void problem(List<String> problems, String text) {
        problems.add(text);
        if (problemsLogged++ < 20) {
            Bridge.info("registry_problem", text);
        }
    }

    private static void writeBlocks(StringBuilder w, List<String> problems, Map<String, Integer> shapes, List<String> shapeList,
            Map<String, Integer> fluids, List<String> fluidList) {
        int expectedState = 0;
        for (int id = 0; id < BuiltInRegistries.BLOCK.size(); id++) {
            Block block = BuiltInRegistries.BLOCK.byId(id);
            var states = block.getStateDefinition().getPossibleStates();
            String name = BuiltInRegistries.BLOCK.getKey(block).toString();
            if (id > 0) w.append(',');
            BlockState def = block.defaultBlockState();
            w.append("{\"name\":").append(str(name));
            w.append(",\"friction\":").append(num(block.getFriction()));
            w.append(",\"speed_factor\":").append(num(block.getSpeedFactor()));
            w.append(",\"jump_factor\":").append(num(block.getJumpFactor()));
            w.append(",\"destroy_time\":").append(num(block.defaultDestroyTime()));
            w.append(",\"requires_tool\":").append(def.requiresCorrectToolForDrops());
            w.append(",\"explosion_resistance\":").append(num(block.getExplosionResistance()));
            String sound = "minecraft:block.stone.break";
            try {
                sound = def.getSoundType().getBreakSound().location().toString();
            } catch (RuntimeException e) {
                problem(problems, name + ": sound type: " + e);
            }
            w.append(",\"sound_type\":").append(str(sound));
            List<Property<?>> props = new ArrayList<>(block.getStateDefinition().getProperties());
            props.sort((a, b) -> a.getName().compareTo(b.getName()));
            w.append(",\"properties\":[");
            for (int p = 0; p < props.size(); p++) {
                if (p > 0) w.append(',');
                w.append("[").append(str(props.get(p).getName())).append(",[");
                appendValues(w, props.get(p));
                w.append("]]");
            }
            w.append("],\"default_state\":").append(states.indexOf(def));
            w.append(",\"states\":[");
            for (int s = 0; s < states.size(); s++) {
                BlockState state = states.get(s);
                if (Block.getId(state) != expectedState + s) {
                    problem(problems, name + ": state id " + Block.getId(state) + " is not " + (expectedState + s));
                }
                if (s > 0) w.append(',');
                writeState(w, name, state, problems, shapes, shapeList, fluids, fluidList);
            }
            w.append("]}");
            expectedState += states.size();
        }
    }

    private static int intern(Map<String, Integer> index, List<String> list, String value) {
        return index.computeIfAbsent(value, v -> {
            list.add(v);
            return list.size() - 1;
        });
    }

    private static <T extends Comparable<T>> void appendValues(StringBuilder w, Property<T> property) {
        boolean first = true;
        for (T value : property.getPossibleValues()) {
            if (!first) w.append(',');
            first = false;
            w.append(str(property.getName(value)));
        }
    }

    /** `[collision, outline, occlusion, light, light block, flags, map colour, sturdy, occluding faces, fluid]`. */
    @SuppressWarnings("deprecation")
    private static void writeState(StringBuilder w, String name, BlockState s, List<String> problems, Map<String, Integer> shapes,
            List<String> shapeList, Map<String, Integer> fluids, List<String> fluidList) {
        BlockPos z = BlockPos.ZERO;
        var g = EmptyBlockGetter.INSTANCE;
        String collision = "[]";
        String outline = "[]";
        String occlusion = "[]";
        int sturdy = 0;
        int occlusionFaces = 0;
        int flags = 0;
        int mapColor = 0;
        try {
            collision = shape(s.getCollisionShape(g, z));
            outline = shape(s.getShape(g, z));
            occlusion = shape(s.getOcclusionShape());
            for (Direction d : Direction.values()) {
                if (s.isFaceSturdy(g, z, d, SupportType.FULL)) sturdy |= 1 << d.ordinal();
                if (s.getFaceOcclusionShape(d).equals(Shapes.block())) occlusionFaces |= 1 << d.ordinal();
            }
            if (s.isAir()) flags |= BridgeAbi.REG_FLAG_AIR;
            if (s.liquid()) flags |= BridgeAbi.REG_FLAG_LIQUID;
            if (s.canBeReplaced()) flags |= BridgeAbi.REG_FLAG_REPLACEABLE;
            if (s.hasBlockEntity()) flags |= BridgeAbi.REG_FLAG_HAS_BLOCK_ENTITY;
            if (s.canOcclude()) flags |= BridgeAbi.REG_FLAG_CAN_OCCLUDE;
            if (s.useShapeForLightOcclusion()) flags |= BridgeAbi.REG_FLAG_USE_SHAPE_FOR_LIGHT_OCCLUSION;
            if (s.isSolidRender()) flags |= BridgeAbi.REG_FLAG_SOLID_RENDER;
            if (s.isCollisionShapeFullBlock(g, z)) flags |= BridgeAbi.REG_FLAG_FULL_COLLISION;
            if (s.propagatesSkylightDown()) flags |= BridgeAbi.REG_FLAG_PROPAGATES_SKYLIGHT_DOWN;
            if (s.isRedstoneConductor(g, z)) flags |= BridgeAbi.REG_FLAG_REDSTONE_CONDUCTOR;
            mapColor = s.getMapColor(g, z).id;
        } catch (RuntimeException e) {
            problem(problems, name + ": " + e);
        }
        int fluid = -1;
        FluidState f = s.getFluidState();
        if (!f.isEmpty()) {
            boolean falling = f.hasProperty(FlowingFluid.FALLING) && f.getValue(FlowingFluid.FALLING);
            fluid = intern(fluids, fluidList, "[" + str(BuiltInRegistries.FLUID.getKey(f.getType()).toString()) + "," + f.getAmount()
                + "," + f.isSource() + "," + falling + "]");
        }
        w.append('[').append(intern(shapes, shapeList, collision))
            .append(',').append(intern(shapes, shapeList, outline))
            .append(',').append(intern(shapes, shapeList, occlusion))
            .append(',').append(s.getLightEmission())
            .append(',').append(s.getLightDampening())
            .append(',').append(flags)
            .append(',').append(mapColor)
            .append(',').append(sturdy)
            .append(',').append(occlusionFaces)
            .append(',').append(fluid)
            .append(']');
    }

    private static String shape(VoxelShape shape) {
        StringBuilder b = new StringBuilder("[");
        List<AABB> boxes = shape.toAabbs();
        for (int i = 0; i < Math.min(boxes.size(), MAX_BOXES); i++) {
            AABB a = boxes.get(i);
            if (i > 0) b.append(',');
            b.append('[').append(num(a.minX)).append(',').append(num(a.minY)).append(',').append(num(a.minZ))
                .append(',').append(num(a.maxX)).append(',').append(num(a.maxY)).append(',').append(num(a.maxZ)).append(']');
        }
        return b.append(']').toString();
    }

    private static void writeItems(StringBuilder w, List<String> problems) {
        for (int id = 0; id < BuiltInRegistries.ITEM.size(); id++) {
            Item item = BuiltInRegistries.ITEM.byId(id);
            if (id > 0) w.append(',');
            w.append("{\"name\":").append(str(BuiltInRegistries.ITEM.getKey(item).toString()));
            DataComponentMap c;
            try {
                c = item.components();
            } catch (RuntimeException unbound) {
                // Before a world's registries are loaded the prototypes are not bound yet (the title report).
                w.append(",\"max_stack_size\":64,\"max_damage\":0}");
                continue;
            }
            w.append(",\"max_stack_size\":").append(item.getDefaultMaxStackSize());
            w.append(",\"max_damage\":").append(c.getOrDefault(DataComponents.MAX_DAMAGE, 0));
            if (item instanceof BlockItem bi) {
                w.append(",\"block\":").append(str(BuiltInRegistries.BLOCK.getKey(bi.getBlock()).toString()));
            }
            Tool tool = c.get(DataComponents.TOOL);
            if (tool != null) {
                w.append(",\"tool\":{\"default_mining_speed\":").append(num(tool.defaultMiningSpeed()));
                w.append(",\"damage_per_block\":").append(tool.damagePerBlock());
                w.append(",\"can_destroy_blocks_in_creative\":").append(tool.canDestroyBlocksInCreative());
                w.append(",\"rules\":[");
                boolean first = true;
                for (Tool.Rule rule : tool.rules()) {
                    for (String blocks : ruleBlocks(rule)) {
                        if (!first) w.append(',');
                        first = false;
                        w.append("[").append(str(blocks)).append(',')
                            .append(rule.speed().map(McrRegistries::num).orElse("null")).append(',')
                            .append(rule.correctForDrops().map(String::valueOf).orElse("null")).append(']');
                    }
                }
                w.append("]}");
            }
            var equippable = c.get(DataComponents.EQUIPPABLE);
            if (equippable != null) {
                String slot = equippable.slot().getSerializedName();
                for (int i = 0; i < EQUIP_SLOTS.length; i++) {
                    if (EQUIP_SLOTS[i].equals(slot)) w.append(",\"equip_slot\":").append(i);
                }
            }
            var consumable = c.get(DataComponents.CONSUMABLE);
            if (consumable != null) w.append(",\"consume_seconds\":").append(num(consumable.consumeSeconds()));
            var cooldown = c.get(DataComponents.USE_COOLDOWN);
            if (cooldown != null) w.append(",\"cooldown_seconds\":").append(num(cooldown.seconds()));
            var food = c.get(DataComponents.FOOD);
            if (food != null) w.append(",\"food\":").append(food.canAlwaysEat());
            w.append(",\"default_components\":[");
            boolean first = true;
            for (TypedDataComponent<?> t : c) {
                int cid = BuiltInRegistries.DATA_COMPONENT_TYPE.getId(t.type());
                if (cid < 0 || cid >= 128) continue;
                if (!first) w.append(',');
                first = false;
                w.append(cid);
            }
            w.append("]}");
        }
    }

    private static Collection<String> ruleBlocks(Tool.Rule rule) {
        List<String> out = new ArrayList<>();
        rule.blocks().unwrap().ifLeft(tag -> out.add("#" + tag.location())).ifRight(list -> {
            for (Holder<Block> h : list) {
                h.unwrapKey().ifPresent(k -> out.add(k.identifier().toString()));
            }
        });
        return out;
    }

    private static void writeEntities(StringBuilder w) {
        for (int id = 0; id < BuiltInRegistries.ENTITY_TYPE.size(); id++) {
            EntityType<?> t = BuiltInRegistries.ENTITY_TYPE.byId(id);
            if (id > 0) w.append(',');
            var d = t.getDimensions();
            w.append("{\"name\":").append(str(BuiltInRegistries.ENTITY_TYPE.getKey(t).toString()))
                .append(",\"width\":").append(num(d.width()))
                .append(",\"height\":").append(num(d.height()))
                .append(",\"eye_height\":").append(num(d.eyeHeight()))
                .append(",\"category\":").append(str(t.getCategory().getName())).append('}');
        }
    }

    private static void writeFluids(StringBuilder w) {
        for (int id = 0; id < BuiltInRegistries.FLUID.size(); id++) {
            Fluid f = BuiltInRegistries.FLUID.byId(id);
            if (id > 0) w.append(',');
            w.append("{\"name\":").append(str(BuiltInRegistries.FLUID.getKey(f).toString()))
                .append(",\"source\":").append(f.isSource(f.defaultFluidState())).append('}');
        }
    }

    private static String num(double v) {
        return Double.isFinite(v) ? Double.toString(v) : "0";
    }

    private static String num(float v) {
        return Float.isFinite(v) ? Float.toString(v) : "0";
    }

    static String str(String s) {
        StringBuilder b = new StringBuilder(s.length() + 2).append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> b.append("\\\"");
                case '\\' -> b.append("\\\\");
                default -> {
                    if (c < 0x20) {
                        b.append(String.format("\\u%04x", (int) c));
                    } else {
                        b.append(c);
                    }
                }
            }
        }
        return b.append('"').toString();
    }
}
