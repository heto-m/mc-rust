package dev.mcr.bridge;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.UUIDUtil;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.network.codec.ByteBufCodecs;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.phys.Vec3;

/**
 * Wire shapes of stream codecs, so Rust can step over values of modded data component and particle types.
 * A shape is prefix notation: leaves {@code bool i8 i16 i32 i64 f32 f64 vi vl str nbt bytes hs item tmpl patch unit},
 * {@code fix N} raw bytes, {@code len} a length-prefixed value, and {@code opt X}, {@code list X}, {@code nlist N X},
 * {@code map K V}, {@code either L R}, {@code holder X}, {@code seq N X1..XN}. Codecs built from anything but
 * vanilla's factories (a mod's own class, {@code StreamCodec.of}, {@code dispatch}) have no shape.
 */
public final class McrCodecs {
    private static final int MAX_DEPTH = 24;
    private static final int MAX_LENGTH = 4096;
    private static Map<Object, String> known;

    private McrCodecs() {}

    /** The shape of {@code codec}, or null when it is not built from vanilla's factories. */
    public static String describe(Object codec) {
        StringBuilder out = new StringBuilder();
        try {
            return write(codec, out, 0) && out.length() <= MAX_LENGTH ? out.toString().trim() : null;
        } catch (ReflectiveOperationException | RuntimeException | LinkageError e) {
            return null;
        }
    }

    /** Class and enclosing method of the first part without a shape, for the report's problems. */
    public static String explain(Object codec) {
        Class<?> c = codec == null ? null : codec.getClass();
        if (c == null) return "null";
        Method m = c.getEnclosingMethod();
        return c.getName() + (m == null ? "" : " in " + m.getDeclaringClass().getSimpleName() + "." + m.getName());
    }

    private static synchronized Map<Object, String> known() {
        if (known != null) return known;
        Map<Object, String> k = new IdentityHashMap<>();
        Map<String, String> byName = Map.ofEntries(
            Map.entry("BOOL", "bool"), Map.entry("BYTE", "i8"), Map.entry("SHORT", "i16"), Map.entry("UNSIGNED_SHORT", "i16"),
            Map.entry("INT", "i32"), Map.entry("VAR_INT", "vi"), Map.entry("LONG", "i64"), Map.entry("VAR_LONG", "vl"),
            Map.entry("FLOAT", "f32"), Map.entry("DOUBLE", "f64"), Map.entry("BYTE_ARRAY", "bytes"), Map.entry("LONG_ARRAY", "list i64"),
            Map.entry("BIT_SET", "bytes"), Map.entry("OPTIONAL_COMPOUND_TAG", "nbt"), Map.entry("VECTOR3F", "seq 3 f32 f32 f32"),
            Map.entry("QUATERNIONF", "seq 4 f32 f32 f32 f32"), Map.entry("CONTAINER_ID", "vi"),
            Map.entry("GAME_PROFILE_PROPERTIES", "list seq 3 str str opt str"), Map.entry("RGB_COLOR", "fix 3"));
        for (Field f : ByteBufCodecs.class.getFields()) {
            String shape = byName.get(f.getName());
            if (shape != null && Modifier.isStatic(f.getModifiers())) {
                try {
                    k.put(f.get(null), shape);
                } catch (IllegalAccessException ignored) {
                    // Interface fields are public.
                }
            }
        }
        k.put(UUIDUtil.STREAM_CODEC, "fix 16");
        k.put(BlockPos.STREAM_CODEC, "i64");
        k.put(Vec3.STREAM_CODEC, "seq 3 f64 f64 f64");
        k.put(ItemStack.OPTIONAL_STREAM_CODEC, "item");
        k.put(ItemStack.STREAM_CODEC, "item");
        k.put(ItemStack.OPTIONAL_LIST_STREAM_CODEC, "list item");
        k.put(ItemStackTemplate.STREAM_CODEC, "tmpl");
        k.put(DataComponentPatch.STREAM_CODEC, "patch");
        known = k;
        return k;
    }

    private static boolean write(Object codec, StringBuilder out, int depth) throws ReflectiveOperationException {
        if (codec == null || depth > MAX_DEPTH || out.length() > MAX_LENGTH) return false;
        String leaf = known().get(codec);
        if (leaf != null) {
            out.append(leaf).append(' ');
            return true;
        }
        Class<?> c = codec.getClass();
        Method m = c.getEnclosingMethod();
        if (m == null) return false;
        Class<?> owner = m.getDeclaringClass();
        String name = m.getName();
        if (owner == StreamCodec.class) {
            switch (name) {
                case "composite" -> {
                    List<Object> parts = new ArrayList<>();
                    for (int i = 1; ; i++) {
                        Field f = field(c, "val$codec" + i);
                        if (f == null) break;
                        parts.add(f.get(codec));
                    }
                    out.append("seq ").append(parts.size()).append(' ');
                    for (Object p : parts) {
                        if (!write(p, out, depth + 1)) return false;
                    }
                    return true;
                }
                case "map", "mapStream" -> {
                    return write(get(c, codec, "this$0"), out, depth + 1);
                }
                case "unit" -> {
                    out.append("unit ");
                    return true;
                }
                default -> {
                    return false;
                }
            }
        }
        if (owner != ByteBufCodecs.class) return false;
        switch (name) {
            case "stringUtf8", "lenientJson" -> out.append("str ");
            case "byteArray" -> out.append("bytes ");
            case "fixedBitSet" -> out.append("fix ").append((-Math.floorDiv(-(int) get(c, codec, "val$size"), 8))).append(' ');
            case "tagCodec", "optionalTagCodec", "compoundTagCodec", "fromCodecWithRegistries" -> out.append("nbt ");
            case "fromCodec" -> {
                return write(get(c, codec, "val$original"), out, depth + 1);
            }
            case "optional" -> {
                out.append("opt ");
                return write(get(c, codec, "val$original"), out, depth + 1);
            }
            case "collection" -> {
                out.append("list ");
                return write(get(c, codec, "val$elementCodec"), out, depth + 1);
            }
            case "fixedSizeCollection" -> {
                out.append("nlist ").append((int) get(c, codec, "val$size")).append(' ');
                return write(get(c, codec, "val$elementCodec"), out, depth + 1);
            }
            case "map" -> {
                out.append("map ");
                return write(get(c, codec, "val$keyCodec"), out, depth + 1) && write(get(c, codec, "val$valueCodec"), out, depth + 1);
            }
            case "either" -> {
                out.append("either ");
                return write(get(c, codec, "val$leftCodec"), out, depth + 1) && write(get(c, codec, "val$rightCodec"), out, depth + 1);
            }
            case "lengthPrefixed" -> out.append("len ");
            case "idMapper", "registry" -> out.append("vi ");
            case "holder" -> {
                out.append("holder ");
                return write(get(c, codec, "val$directCodec"), out, depth + 1);
            }
            case "holderSet" -> out.append("hs ");
            default -> {
                return false;
            }
        }
        return true;
    }

    private static Field field(Class<?> c, String name) {
        try {
            Field f = c.getDeclaredField(name);
            f.setAccessible(true);
            return f;
        } catch (NoSuchFieldException e) {
            return null;
        }
    }

    private static Object get(Class<?> c, Object codec, String name) throws ReflectiveOperationException {
        Field f = field(c, name);
        if (f == null) throw new NoSuchFieldException(name);
        return f.get(codec);
    }
}
