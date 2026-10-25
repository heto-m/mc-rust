package dev.mcr.oracle.physics;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.authlib.GameProfile;
import dev.mcr.oracle.Boot;
import dev.mcr.oracle.Harness;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Predicate;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.client.multiplayer.PlayerInfo;
import net.minecraft.client.player.KeyboardInput;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import net.minecraft.client.sounds.SoundManager;
import net.minecraft.client.tutorial.Tutorial;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.protocol.Packet;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.attribute.EnvironmentAttributeSystem;
import net.minecraft.world.damagesource.DamageSources;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/** {@code LocalPlayer.tick} in a scripted block scene with scripted keys; see mcr-parity's player_physics.rs. */
public final class PlayerPhysicsHarness implements Harness {
    private static Map<Long, BlockState> blocks = new HashMap<>();
    private static int floorY;
    private static int floorExtent;
    private static BlockState floor = Blocks.AIR.defaultBlockState();
    private static Map<Long, LevelChunk> chunks = new HashMap<>();
    private static final WorldBorder BORDER = new WorldBorder();
    private static DamageSources damage;
    /** No layers: every attribute at its default, which is the overworld's (slow lava, no ultrawarm). */
    private static final EnvironmentAttributeSystem ATTRIBUTES = EnvironmentAttributeSystem.builder().build();

    private boolean reported;

    static BlockState sceneBlock(int x, int y, int z) {
        BlockState state = blocks.get(BlockPos.asLong(x, y, z));
        if (state != null) {
            return state;
        }
        boolean onFloor = y == floorY && Math.abs(x) <= floorExtent && Math.abs(z) <= floorExtent;
        return onFloor ? floor : Blocks.AIR.defaultBlockState();
    }

    private final Minecraft minecraft;
    private final Options options;
    private final Camera camera;
    private final RegistryAccess.Frozen registries;

    /** Answers block, fluid and collision queries from the case's scene; nothing else exists. */
    static final class SceneLevel extends ClientLevel {
        SceneLevel() {
            super(null, null, null, null, 0, 0, null, false, 0L, 0);
        }

        @Override
        public BlockState getBlockState(BlockPos pos) {
            return sceneBlock(pos.getX(), pos.getY(), pos.getZ());
        }

        @Override
        public FluidState getFluidState(BlockPos pos) {
            return getBlockState(pos).getFluidState();
        }

        @Override
        public BlockGetter getChunkForCollisions(int chunkX, int chunkZ) {
            return this;
        }

        @Override
        public ChunkAccess getChunk(int chunkX, int chunkZ, ChunkStatus status, boolean loadOrGenerate) {
            return chunks.computeIfAbsent(ChunkPos.pack(chunkX, chunkZ), k -> sceneChunk(this, chunkX, chunkZ));
        }

        @Override
        public void playSeededSound(Entity except, double x, double y, double z, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed) {
        }

        @Override
        public void playSeededSound(Entity except, Entity sourceEntity, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed) {
        }

        @Override
        public void playLocalSound(Entity sourceEntity, SoundEvent sound, SoundSource source, float volume, float pitch) {
        }

        @Override
        public void playLocalSound(double x, double y, double z, SoundEvent sound, SoundSource source, float volume, float pitch, boolean distanceDelay) {
        }

        @Override
        public void playPlayerSound(SoundEvent sound, SoundSource source, float volume, float pitch) {
        }

        @Override
        public void addParticle(ParticleOptions particle, double x, double y, double z, double xd, double yd, double zd) {
        }

        @Override
        public void addParticle(ParticleOptions particle, boolean overrideLimiter, boolean alwaysShow, double x, double y, double z, double xd, double yd, double zd) {
        }

        @Override
        public void addAlwaysVisibleParticle(ParticleOptions particle, double x, double y, double z, double xd, double yd, double zd) {
        }

        @Override
        public void addAlwaysVisibleParticle(ParticleOptions particle, boolean overrideLimiter, double x, double y, double z, double xd, double yd, double zd) {
        }

        @Override
        public void levelEvent(Entity source, int type, BlockPos pos, int data) {
        }

        @Override
        public void gameEvent(Holder<GameEvent> gameEvent, Vec3 pos, GameEvent.Context context) {
        }

        @Override
        public List<Entity> getPushableEntities(Entity pusher, AABB bb) {
            return List.of();
        }

        @Override
        public EnvironmentAttributeSystem environmentAttributes() {
            return ATTRIBUTES;
        }

        @Override
        public RegistryAccess registryAccess() {
            return Boot.registryAccess();
        }

        @Override
        public DamageSources damageSources() {
            return damage;
        }

        @Override
        public boolean isRainingAt(BlockPos pos) {
            return false;
        }

        @Override
        public WorldBorder getWorldBorder() {
            return BORDER;
        }

        @Override
        public boolean hasChunk(int chunkX, int chunkZ) {
            return true;
        }

        @Override
        public int getMinY() {
            return -64;
        }

        @Override
        public int getHeight() {
            return 384;
        }

        @Override
        public boolean isClientSide() {
            return true;
        }

        @Override
        public List<Entity> getEntities(Entity except, AABB bb, Predicate<? super Entity> selector) {
            return List.of();
        }

        @Override
        public FeatureFlagSet enabledFeatures() {
            return FeatureFlags.VANILLA_SET;
        }
    }

    /** Plays nothing: underwater ambience and elytra wind are sounds, not movement. */
    static final class SilentSounds extends SoundManager {
        SilentSounds() {
            super(null);
        }

        @Override
        public SoundEngine.PlayResult play(SoundInstance instance) {
            return SoundEngine.PlayResult.NOT_STARTED;
        }

        @Override
        public void playDelayed(SoundInstance instance, int delay) {
        }

        @Override
        public void stop(SoundInstance instance) {
        }
    }

    /** Sends nothing; the local profile and loaded state are all the player asks of it. */
    static final class SilentConnection extends ClientPacketListener {
        SilentConnection() {
            super(null, null, null);
        }

        @Override
        public void send(Packet<?> packet) {
        }

        @Override
        public GameProfile getLocalGameProfile() {
            return new GameProfile(new UUID(0L, 1L), "mcr");
        }

        @Override
        public boolean hasClientLoaded() {
            return true;
        }

        @Override
        public PlayerInfo getPlayerInfo(UUID player) {
            return null;
        }
    }

    public PlayerPhysicsHarness(Boot boot) {
        Boot.tags();
        registries = Boot.registryAccess();
        damage = new DamageSources(registries);
        try {
            minecraft = Boot.allocate(Minecraft.class);
            set(Minecraft.class, "fixerUpper", minecraft, DataFixers.getDataFixer());
            Field instance = Minecraft.class.getDeclaredField("instance");
            instance.setAccessible(true);
            instance.set(null, minecraft);
            Path dir = Files.createTempDirectory("mcr-physics");
            dir.toFile().deleteOnExit();
            options = new Options(minecraft, dir.toFile());
            set(Minecraft.class, "options", minecraft, options);
            set(Minecraft.class, "gui", minecraft, Boot.allocate(Gui.class));
            set(Minecraft.class, "tutorial", minecraft, Boot.allocate(Tutorial.class));
            set(Minecraft.class, "gameMode", minecraft, Boot.allocate(MultiPlayerGameMode.class));
            set(Minecraft.class, "soundManager", minecraft, Boot.allocate(SilentSounds.class));
            camera = Boot.allocate(Camera.class);
            GameRenderer renderer = Boot.allocate(GameRenderer.class);
            set(GameRenderer.class, "mainCamera", renderer, camera);
            set(Minecraft.class, "gameRenderer", minecraft, renderer);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("fake client", e);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    static void set(Class<?> owner, String name, Object target, Object value) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(name);
        f.setAccessible(true);
        f.set(target, value);
    }

    /** Reads the scene at its own origin; claims fluid and every block so scans fall through to it. */
    static final class SceneSection extends LevelChunkSection {
        int x;
        int y;
        int z;

        SceneSection() {
            super(null, null);
        }

        @Override
        public BlockState getBlockState(int sectionX, int sectionY, int sectionZ) {
            return sceneBlock(x + sectionX, y + sectionY, z + sectionZ);
        }

        @Override
        public FluidState getFluidState(int sectionX, int sectionY, int sectionZ) {
            return getBlockState(sectionX, sectionY, sectionZ).getFluidState();
        }

        @Override
        public boolean maybeHas(Predicate<BlockState> predicate) {
            return true;
        }

        @Override
        public boolean hasFluid() {
            return true;
        }

        @Override
        public boolean hasOnlyAir() {
            return false;
        }
    }

    private static LevelChunk sceneChunk(SceneLevel level, int chunkX, int chunkZ) {
        try {
            LevelChunk c = Boot.allocate(LevelChunk.class);
            LevelChunkSection[] sections = new LevelChunkSection[level.getSectionsCount()];
            for (int i = 0; i < sections.length; i++) {
                SceneSection section = Boot.allocate(SceneSection.class);
                section.x = chunkX << 4;
                section.y = (level.getMinSectionY() + i) << 4;
                section.z = chunkZ << 4;
                sections[i] = section;
            }
            set(ChunkAccess.class, "sections", c, sections);
            set(ChunkAccess.class, "levelHeightAccessor", c, level);
            set(ChunkAccess.class, "chunkPos", c, new ChunkPos(chunkX, chunkZ));
            return c;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("scene chunk", e);
        }
    }

    private LocalPlayer spawn(ClientLevel level, Vec3 start) throws ReflectiveOperationException {
        SilentConnection connection = Boot.allocate(SilentConnection.class);
        LocalPlayer player = new LocalPlayer(minecraft, level, connection, null, null, net.minecraft.world.entity.player.Input.EMPTY, false, null, null);
        Field sounds = LocalPlayer.class.getDeclaredField("ambientSoundHandlers");
        sounds.setAccessible(true);
        ((List<?>) sounds.get(player)).clear();
        player.input = new KeyboardInput(options);
        player.setPos(start);
        player.setId(1);
        minecraft.player = player;
        camera.setEntity(player);
        return player;
    }

    @Override
    public JsonElement run(JsonObject testCase) {
        blocks = new HashMap<>();
        JsonArray f = testCase.getAsJsonArray("floor");
        floorY = f.get(0).getAsInt();
        floor = Block.stateById(f.get(1).getAsInt());
        floorExtent = f.get(2).getAsInt();
        for (JsonElement b : testCase.getAsJsonArray("blocks")) {
            JsonArray a = b.getAsJsonArray();
            blocks.put(BlockPos.asLong(a.get(0).getAsInt(), a.get(1).getAsInt(), a.get(2).getAsInt()), Block.stateById(a.get(3).getAsInt()));
        }
        JsonObject out = new JsonObject();
        JsonArray pos = new JsonArray();
        JsonArray vel = new JsonArray();
        JsonArray ground = new JsonArray();
        try {
            SceneLevel level = Boot.allocate(SceneLevel.class);
            chunks = new HashMap<>();
            JsonArray start = testCase.getAsJsonArray("start");
            LocalPlayer player = spawn(level, new Vec3(start.get(0).getAsDouble(), start.get(1).getAsDouble(), start.get(2).getAsDouble()));
            if (testCase.get("elytra").getAsBoolean()) {
                player.setItemSlot(EquipmentSlot.CHEST, new ItemStack(Items.ELYTRA));
            }
            for (JsonElement t : testCase.getAsJsonArray("ticks")) {
                JsonObject tick = t.getAsJsonObject();
                String keys = tick.get("keys").getAsString();
                options.keyUp.setDown(keys.indexOf('F') >= 0);
                options.keyDown.setDown(keys.indexOf('B') >= 0);
                options.keyLeft.setDown(keys.indexOf('L') >= 0);
                options.keyRight.setDown(keys.indexOf('R') >= 0);
                options.keyJump.setDown(keys.indexOf('J') >= 0);
                options.keyShift.setDown(keys.indexOf('S') >= 0);
                options.keySprint.setDown(keys.indexOf('P') >= 0);
                player.setYRot(tick.get("yaw").getAsFloat());
                player.setXRot(tick.get("pitch").getAsFloat());
                player.tick();
                // The client sends position right after the tick; sending also re-reads the auto-jump option.
                player.sendChanges();
                Vec3 p = player.position();
                Vec3 v = player.getDeltaMovement();
                pos.add(triple(p.x, p.y, p.z));
                vel.add(triple(v.x, v.y, v.z));
                ground.add(player.onGround());
            }
        } catch (ReflectiveOperationException | RuntimeException e) {
            if (!reported) {
                reported = true;
                e.printStackTrace();
            }
            out.addProperty("error", e.getClass().getSimpleName());
        }
        out.add("pos", pos);
        out.add("vel", vel);
        out.add("ground", ground);
        return out;
    }

    private static JsonArray triple(double a, double b, double c) {
        JsonArray out = new JsonArray();
        out.add(a);
        out.add(b);
        out.add(c);
        return out;
    }
}
