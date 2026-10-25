package dev.mcr.bridge;

import dev.mcr.boot.Abi.Flavour;

/** Mirror of mcr-modbridge's extension table and messages. Entries are append-only; Rust tests compare both. */
public final class BridgeAbi {
    public static final int VERSION = 22;
    public static final String TABLE_PROPERTY = "mcr.bridge.table";
    public static final String COMPOSE_PROPERTY = "mcr.compose";

    public static final int SHARED_CONTROL_RING = 0;
    public static final int SHARED_CONTROL_BELL = 1;
    public static final int SHARED_NET_IN_RING = 2;
    public static final int SHARED_NET_OUT_RING = 3;
    public static final int SHARED_SOUND_RING = 4;
    public static final int SHARED_SOUND_STATES = 5;
    public static final int SHARED_SOUND_CONTROL = 6;
    public static final int SHARED_FRAME_STATE = 16;
    public static final int SHARED_FRAME_BELL = 17;
    public static final int SHARED_INPUT_STATE = 18;
    public static final int SHARED_SECTIONS = 19;
    public static final int SHARED_LIGHTS = 20;

    public static final int MSG_FRAME = 1;
    public static final int MSG_SHUTDOWN = 2;
    public static final int MSG_INPUT = 3;
    public static final int MSG_KEYS = 4;
    public static final int MSG_PACKS = 5;
    public static final int MSG_WORLD = 6;
    public static final int MSG_MODELS = 7;
    public static final int WORLD_STATUS = 1;
    public static final int WORLD_EULA = 2;
    public static final int WORLD_READY = 3;
    public static final int WORLD_FAILED = 4;
    public static final int WORLD_STOPPED = 5;
    public static final int WORLD_LAN = 6;
    public static final int WORLD_LOCKED = 7;
    public static final int SYNC_MAX_LINES = 256;

    public static final int IN_TYPE = 0;
    public static final int IN_A = 4;
    public static final int IN_B = 8;
    public static final int IN_C = 12;
    public static final int IN_D = 16;
    public static final int IN_X = 24;
    public static final int IN_Y = 32;
    public static final int IN_DX = 40;
    public static final int IN_DY = 48;
    public static final int IN_TEXT_DATA = 56;
    public static final int IN_KEY = 1;
    public static final int IN_TEXT = 2;
    public static final int IN_PREEDIT = 3;
    public static final int IN_MOTION = 4;
    public static final int IN_BUTTON = 5;
    public static final int IN_WHEEL = 6;
    public static final int IN_DROP = 7;
    public static final int INPUT_MAX_TEXT = 4096;
    public static final int INPUT_MAX_CLIPBOARD = 1048576;

    public static final int ISTATE_KEYS = 0;
    public static final int ISTATE_MODS = 64;
    public static final int ISTATE_RUST_FLAGS = 68;
    public static final int ISTATE_JAVA_FLAGS = 72;
    public static final int ISTATE_GUI_SCALE = 76;
    public static final int ISTATE_CURSOR = 80;
    public static final int ISTATE_NATIVE_SEQ = 84;
    public static final int ISTATE_NATIVE_KIND = 88;
    public static final int ISTATE_SCREEN_MOVES = 92;
    public static final int ISTATE_TEXT_AREA = 96;
    public static final int ISTATE_SIZE = 128;
    public static final int RUST_FOCUSED = 1;
    public static final int JAVA_SCREEN = 1;
    public static final int JAVA_OVERLAY = 2;
    public static final int JAVA_GRABBED = 4;
    public static final int JAVA_TEXT_INPUT = 8;
    public static final int JAVA_QUIT = 16;
    public static final int JAVA_PAUSING = 32;
    public static final int MOVE_FORWARD = 1;
    public static final int MOVE_BACKWARD = 2;
    public static final int MOVE_LEFT = 4;
    public static final int MOVE_RIGHT = 8;
    public static final int MOVE_JUMP = 16;
    public static final int MOVE_SNEAK = 32;
    public static final int MOVE_SPRINT = 64;

    public static final int FRAME_INDEX = 0;
    public static final int FRAME_TICKS = 8;
    public static final int FRAME_WIDTH = 12;
    public static final int FRAME_HEIGHT = 16;
    public static final int FRAME_PARTIAL_TICK = 20;
    public static final int FRAME_SCALE = 24;
    public static final int FRAME_FLAGS = 28;
    public static final int FRAME_CAMERA = 32;
    public static final int FRAME_YAW = 56;
    public static final int FRAME_PITCH = 60;
    public static final int FRAME_VIEW = 64;
    public static final int FRAME_PROJ = 128;
    public static final int FRAME_RENDER_DISTANCE = 192;
    public static final int FRAME_ENTITY_SCALE = 196;
    public static final int FRAME_RENDER_CAMERA = 200;
    public static final int FRAME_RENDER_YAW = 224;
    public static final int FRAME_RENDER_PITCH = 228;
    public static final int FRAME_CAMERA_DISTANCE = 232;
    public static final int FRAME_SIZE = 236;
    public static final int FRAME_LOCKSTEP = 1;
    public static final int FRAME_DETACHED = 2;
    public static final int FRAME_CAMERA_HANDOFF = 4;
    public static final int FRAME_SECTIONS = 8;

    public static final int FSTATE_DONE = 0;
    public static final int FSTATE_TICKS = 8;
    public static final int FSTATE_NANOS = 16;
    public static final int FSTATE_FLAGS = 24;
    public static final int FSTATE_WIDTH = 28;
    public static final int FSTATE_HEIGHT = 32;
    public static final int FSTATE_FOV_SCALE = 36;
    public static final int FSTATE_CAMERA = 40;
    public static final int FSTATE_CAMERA_POS = 48;
    public static final int FSTATE_CAMERA_YAW = 72;
    public static final int FSTATE_CAMERA_PITCH = 76;
    public static final int FSTATE_PROF = 80;
    public static final int FSTATE_CAMERA_ROLL = 112;
    public static final int FSTATE_PERSPECTIVE = 116;
    public static final int FSTATE_VIEW_ROLL = 120;
    public static final int FSTATE_SIZE = 128;
    public static final int PERSPECTIVE_NONE = 0;
    public static final int PERSPECTIVE_FIRST = 1;
    public static final int PERSPECTIVE_THIRD_BACK = 2;
    public static final int PERSPECTIVE_THIRD_FRONT = 3;
    public static final int PROF_PRE = 0;
    public static final int PROF_TICK = 1;
    public static final int PROF_EXTRACT = 2;
    public static final int PROF_RENDER = 3;
    public static final int PROF_WORLD = 4;
    public static final int PROF_GUI = 5;
    public static final int PROF_FRAME = 6;
    public static final int PROF_COUNT = 8;
    public static final int FSTATE_READY = 1;
    public static final int FSTATE_LOADED = 2;
    public static final int FSTATE_WEATHER = 4;
    public static final int FSTATE_STEPS = 8;
    public static final int FSTATE_TURN = 16;
    public static final int FSTATE_WAITING = 32;
    public static final int CAMERA_MOD = 1;
    public static final int CAMERA_DETACHED = 2;
    public static final int CAMERA_INPUT = 4;
    public static final int SEC_SEQ = 0;
    public static final int SEC_ORIGIN = 8;
    public static final int SEC_DIMS = 20;
    public static final int SEC_BITS = 64;
    public static final int SEC_MAX_BITS = 8388608;
    public static final int LIGHT_SEQ = 0;
    public static final int LIGHT_FLAGS = 8;
    public static final int LIGHT_COUNT = 12;
    public static final int LIGHT_ENTRIES = 16;
    public static final int LIGHT_ENTRY_SIZE = 40;
    public static final int LIGHT_START = 0;
    public static final int LIGHT_END = 24;
    public static final int LIGHT_LEVEL = 36;
    public static final int LIGHT_MAX = 256;
    public static final int LIGHTS_PROVIDER = 1;
    public static final int LIGHTS_ENABLED = 2;

    public static final int SLOT_IMMEDIATE = 0;
    public static final int SLOT_GUI = 1;
    public static final int SLOT_WORLD_SOLID = 2;
    public static final int SLOT_WORLD_TRANSLUCENT = 3;
    public static final int SLOT_WORLD_AFTER = 4;
    public static final int SLOT_POST_WORLD = 5;

    public static final int REPORT_MOD_LOADED = 1;
    public static final int REPORT_INIT = 2;
    public static final int REPORT_SCREEN = 3;
    public static final int REPORT_CRASH = 4;
    public static final int REPORT_INFO = 5;
    public static final int REPORT_CONNECT = 6;
    public static final int REPORT_STOPPING = 7;
    public static final int REPORT_KEYS = 8;
    public static final int REPORT_WORLD = 9;

    /** State flag bits of the registry report, mcr-data's `flag` module. */
    public static final int REG_FLAG_AIR = 1;
    public static final int REG_FLAG_LIQUID = 2;
    public static final int REG_FLAG_REPLACEABLE = 4;
    public static final int REG_FLAG_HAS_BLOCK_ENTITY = 8;
    public static final int REG_FLAG_CAN_OCCLUDE = 16;
    public static final int REG_FLAG_USE_SHAPE_FOR_LIGHT_OCCLUSION = 32;
    public static final int REG_FLAG_SOLID_RENDER = 64;
    public static final int REG_FLAG_FULL_COLLISION = 128;
    public static final int REG_FLAG_PROPAGATES_SKYLIGHT_DOWN = 256;
    public static final int REG_FLAG_REDSTONE_CONDUCTOR = 512;
    public static final int REG_FLAG_LEGACY_SOLID = 1024;

    public static final int PHASE_LOGIN = 1;
    public static final int PHASE_CONFIGURATION = 2;
    public static final int PHASE_PLAY = 3;

    public static final int NET_HEADER = 8;
    public static final int NET_TAG = 0;
    public static final int NET_SESSION = 4;

    public static final int NET_FRAME = 1;
    public static final int NET_OPEN = 2;
    public static final int NET_CLOSE = 3;
    public static final int NET_PLAYER = 4;
    public static final int NET_PROBE = 5;
    public static final int NET_ACTION = 6;

    public static final int OUT_FRAME = 1;
    public static final int OUT_CHAT = 2;
    public static final int OUT_DISCONNECT = 3;
    public static final int OUT_PROBE = 4;
    public static final int DISCONNECT_LEFT = 0;
    public static final int DISCONNECT_FAILED = 1;

    public static final int CHAT_MESSAGE = 0;
    public static final int CHAT_COMMAND = 1;

    public static final int OPEN_PORT = 8;
    public static final int OPEN_HOST = 12;

    public static final int PLAYER_TICK = 8;
    public static final int PLAYER_POS = 16;
    public static final int PLAYER_OLD_POS = 40;
    public static final int PLAYER_VELOCITY = 64;
    public static final int PLAYER_YAW = 88;
    public static final int PLAYER_PITCH = 92;
    public static final int PLAYER_FLAGS = 96;
    public static final int PLAYER_POSE = 100;
    public static final int PLAYER_SLOT = 104;
    public static final int PLAYER_VEHICLE = 108;
    public static final int PLAYER_INPUT = 112;
    public static final int PLAYER_SIZE = 120;

    public static final int PLAYER_ON_GROUND = 1;
    public static final int PLAYER_SHIFT = 2;
    public static final int PLAYER_SPRINT = 4;
    public static final int PLAYER_SWIM = 8;
    public static final int PLAYER_FALL_FLYING = 16;

    public static final int INPUT_FORWARD = 1;
    public static final int INPUT_BACKWARD = 2;
    public static final int INPUT_LEFT = 4;
    public static final int INPUT_RIGHT = 8;
    public static final int INPUT_JUMP = 16;
    public static final int INPUT_SHIFT = 32;
    public static final int INPUT_SPRINT = 64;

    public static final int ACTION_KIND = 8;
    public static final int ACTION_ARG = 12;
    public static final int ACTION_POS = 16;
    public static final int ACTION_VALUE = 28;
    public static final int ACTION_SIZE = 32;
    public static final int ACTION_SWING = 1;
    public static final int ACTION_DROP = 2;
    public static final int ACTION_DESTROY = 3;
    public static final int ACTION_RESET_ATTACK = 4;
    public static final int SWING_OFF_HAND = 1;
    public static final int SWING_ATTACK = 2;

    public static final int PROBE_REQUEST = 8;
    public static final int PROBE_CENTER_X = 12;
    public static final int PROBE_CENTER_Z = 16;
    public static final int PROBE_RADIUS = 20;
    public static final int PROBE_FLAGS = 24;
    public static final int PROBE_COUNT = 28;
    public static final int PROBE_POSITIONS = 32;
    public static final int PROBE_GC = 1;
    public static final int PROBE_DISCONNECT = 2;

    public static final int ANSWER_REQUEST = 8;
    public static final int ANSWER_FLAGS = 12;
    public static final int ANSWER_PLAYER_TICK = 16;
    public static final int ANSWER_POS = 24;
    public static final int ANSWER_HEAP = 48;
    public static final int ANSWER_CHUNK_PACKETS = 56;
    public static final int ANSWER_CHUNK_NANOS = 64;
    public static final int ANSWER_FRAMES = 72;
    public static final int ANSWER_FEED_NANOS = 80;
    public static final int ANSWER_CHUNKS = 88;
    public static final int ANSWER_BLOCKS = 92;
    public static final int ANSWER_ENTITIES = 96;
    public static final int ANSWER_HELD_COUNT = 100;
    public static final int ANSWER_SWINGS = 104;
    public static final int ANSWER_TUTORIAL = 108;
    public static final int ANSWER_DATA = 112;
    public static final int ANSWER_LEVEL = 1;
    public static final int ANSWER_PLAYER = 2;

    public static final int PIPELINE_MAGIC = 1347568461;
    public static final int PIPELINE_FORMAT = 1;

    public static final int GPU_MAX_ARGS = 32;
    public static final int GPU_INFO_SLOTS = 6;
    public static final int GPU_FEATURE_POLYGON_LINE = 1;
    public static final int GPU_FEATURE_FIRST_INSTANCE = 2;
    public static final int GPU_MAX_UNIFORMS = 16;

    public static final int D_INFO = 1;
    public static final int D_BUFFER = 2;
    public static final int D_TRANSIENT = 3;
    public static final int D_HOST = 4;
    public static final int D_TEXTURE = 5;
    public static final int D_VIEW = 6;
    public static final int D_SAMPLER = 7;
    public static final int D_PIPELINE = 8;
    public static final int D_DESTROY = 9;
    public static final int D_ENCODER = 10;
    public static final int D_SURFACE = 11;
    public static final int D_SURFACE_CONFIGURE = 12;
    public static final int D_SURFACE_PRESENT = 13;
    public static final int D_STATS = 14;
    public static final int D_SLOT = 15;
    public static final int D_LEVEL_VIEW = 16;
    public static final int GPU_TEXTURE_VIRTUAL = 1073741824;

    public static final int E_CLEAR = 1;
    public static final int E_CLEAR_REGION = 2;
    public static final int E_WRITE_BUFFER = 3;
    public static final int E_COPY_BUFFER = 4;
    public static final int E_WRITE_TEXTURE = 5;
    public static final int E_BUFFER_TO_TEXTURE = 6;
    public static final int E_TEXTURE_TO_BUFFER = 7;
    public static final int E_TEXTURE_TO_TEXTURE = 8;
    public static final int E_FLUSH_MAPPED = 9;
    public static final int E_BLIT = 10;
    public static final int E_DIRTY = 11;
    public static final int E_SUBMIT = 12;
    public static final int E_POLL = 13;
    public static final int E_WAIT = 14;
    public static final int E_SPLIT = 15;

    public static final int P_PIPELINE = 1;
    public static final int P_UNIFORM_BUFFER = 2;
    public static final int P_UNIFORM_TEXTURE = 3;
    public static final int P_UNIFORM_NONE = 4;
    public static final int P_PUSH = 5;
    public static final int P_SCISSOR = 6;
    public static final int P_NO_SCISSOR = 7;
    public static final int P_VERTEX = 8;
    public static final int P_INDEX = 9;
    public static final int P_DRAW = 10;
    public static final int P_DRAW_INDEXED = 11;
    public static final int P_MULTI_DRAW = 12;
    public static final int P_MULTI_DRAW_INDEXED = 13;
    public static final int P_DRAW_INDIRECT = 14;
    public static final int P_DRAW_INDEXED_INDIRECT = 15;
    public static final int P_DEBUG_PUSH = 16;
    public static final int P_DEBUG_POP = 17;
    public static final int PASS_CONTINUES = 1;
    public static final int PASS_RESUMES = 2;

    /** Renderpearl enum constants in ordinal order; the backend sends ordinals and checks these names at boot. */
    public static final String[] GPU_FORMATS = {
        "R8_UNORM", "R8_SNORM", "RG8_UNORM", "RG8_SNORM", "RGB8_UNORM", "RGB8_SNORM", "RGBA8_UNORM", "RGBA8_SNORM",
        "R16_UNORM", "R16_SNORM", "RG16_UNORM", "RG16_SNORM", "RGB16_UNORM", "RGB16_SNORM", "RGBA16_UNORM", "RGBA16_SNORM",
        "R8_UINT", "R8_SINT", "RG8_UINT", "RG8_SINT", "RGB8_UINT", "RGB8_SINT", "RGBA8_UINT", "RGBA8_SINT",
        "R16_UINT", "R16_SINT", "RG16_UINT", "RG16_SINT", "RGB16_UINT", "RGB16_SINT", "RGBA16_UINT", "RGBA16_SINT",
        "R32_UINT", "R32_SINT", "RG32_UINT", "RG32_SINT", "RGB32_UINT", "RGB32_SINT", "RGBA32_UINT", "RGBA32_SINT",
        "R16_FLOAT", "RG16_FLOAT", "RGB16_FLOAT", "RGBA16_FLOAT", "R32_FLOAT", "RG32_FLOAT", "RGB32_FLOAT", "RGBA32_FLOAT",
        "RGB10A2_UNORM", "RGB10A2_UINT", "RG11B10_FLOAT", "D32_FLOAT", "D32_FLOAT_S8_UINT", "D24_UNORM_S8_UINT", "D16_UNORM",
        "S8_UINT"
    };
    public static final String[] BLEND_FACTORS = {
        "CONSTANT_ALPHA", "CONSTANT_COLOR", "DST_ALPHA", "DST_COLOR", "ONE", "ONE_MINUS_CONSTANT_ALPHA",
        "ONE_MINUS_CONSTANT_COLOR", "ONE_MINUS_DST_ALPHA", "ONE_MINUS_DST_COLOR", "ONE_MINUS_SRC_ALPHA",
        "ONE_MINUS_SRC_COLOR", "SRC_ALPHA", "SRC_ALPHA_SATURATE", "SRC_COLOR", "ZERO"
    };
    public static final String[] BLEND_OPS = {"ADD", "SUBTRACT", "REVERSE_SUBTRACT", "MIN", "MAX"};
    public static final String[] COMPARE_OPS = {
        "ALWAYS_PASS", "LESS_THAN", "LESS_THAN_OR_EQUAL", "EQUAL", "NOT_EQUAL", "GREATER_THAN_OR_EQUAL", "GREATER_THAN",
        "NEVER_PASS"
    };
    public static final String[] POLYGON_MODES = {"FILL", "WIREFRAME"};
    public static final String[] TOPOLOGIES = {
        "LINES", "DEBUG_LINES", "DEBUG_LINE_STRIP", "POINTS", "TRIANGLES", "TRIANGLE_STRIP", "TRIANGLE_FAN", "QUADS"
    };
    public static final String[] UNIFORM_TYPES = {"COMBINED_IMAGE_SAMPLER", "UNIFORM_BUFFER", "TEXEL_BUFFER"};
    public static final String[] ADDRESS_MODES = {"REPEAT", "CLAMP_TO_EDGE"};
    public static final String[] FILTER_MODES = {"NEAREST", "LINEAR"};

    public enum Entry {
        SHARED_ADDRESS("shared_address", "(I)J", Flavour.CRITICAL),
        REPORT("report", "(IJI)V", Flavour.NORMAL),
        GPU_DEVICE("gpu_device", "(IJI)J", Flavour.NORMAL),
        GPU_ENCODE("gpu_encode", "(JIJI)J", Flavour.NORMAL),
        GPU_PASS("gpu_pass", "(JJIJI)J", Flavour.NORMAL),
        GPU_ERROR("gpu_error", "(JI)I", Flavour.NORMAL),
        SOUND_UPLOAD("sound_upload", "(JIIII)I", Flavour.NORMAL),
        SOUND_FREE("sound_free", "(I)V", Flavour.NORMAL),
        SOUND_STREAM_OPEN("sound_stream_open", "(IIIII)I", Flavour.NORMAL),
        SOUND_STREAM_PUSH("sound_stream_push", "(IIJI)I", Flavour.NORMAL),
        CLIPBOARD_GET("clipboard_get", "(JI)I", Flavour.NORMAL),
        CLIPBOARD_SET("clipboard_set", "(JI)V", Flavour.NORMAL),
        REGISTRY_REPORT("registry_report", "(JI)V", Flavour.NORMAL),
        RENDER_REPORT("render_report", "(JI)V", Flavour.NORMAL);

        public final String symbol;
        public final String descriptor;
        public final Flavour flavour;

        Entry(String symbol, String descriptor, Flavour flavour) {
            this.symbol = symbol;
            this.descriptor = descriptor;
            this.flavour = flavour;
        }
    }

    private BridgeAbi() {}
}
