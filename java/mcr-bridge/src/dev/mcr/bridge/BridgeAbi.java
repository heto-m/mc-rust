package dev.mcr.bridge;

import dev.mcr.boot.Abi.Flavour;

/** Mirror of mcr-modbridge's extension table and messages. Entries are append-only; Rust tests compare both. */
public final class BridgeAbi {
    public static final int VERSION = 2;
    public static final String TABLE_PROPERTY = "mcr.bridge.table";

    public static final int SHARED_CONTROL_RING = 0;
    public static final int SHARED_CONTROL_BELL = 1;
    public static final int SHARED_NET_IN_RING = 2;
    public static final int SHARED_NET_OUT_RING = 3;

    public static final int MSG_FRAME = 1;
    public static final int MSG_SHUTDOWN = 2;

    public static final int FRAME_INDEX = 0;
    public static final int FRAME_TICKS = 8;
    public static final int FRAME_WIDTH = 12;
    public static final int FRAME_HEIGHT = 16;
    public static final int FRAME_PARTIAL_TICK = 20;
    public static final int FRAME_SCALE = 24;
    public static final int FRAME_SIZE = 32;

    public static final int REPORT_MOD_LOADED = 1;
    public static final int REPORT_INIT = 2;
    public static final int REPORT_SCREEN = 3;
    public static final int REPORT_CRASH = 4;
    public static final int REPORT_INFO = 5;
    public static final int REPORT_CONNECT = 6;
    public static final int REPORT_STOPPING = 7;

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

    public static final int OUT_FRAME = 1;
    public static final int OUT_CHAT = 2;
    public static final int OUT_DISCONNECT = 3;
    public static final int OUT_PROBE = 4;

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
    public static final int PLAYER_SIZE = 112;

    public static final int PLAYER_ON_GROUND = 1;
    public static final int PLAYER_SHIFT = 2;
    public static final int PLAYER_SPRINT = 4;
    public static final int PLAYER_SWIM = 8;
    public static final int PLAYER_FALL_FLYING = 16;

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
    public static final int ANSWER_DATA = 104;
    public static final int ANSWER_LEVEL = 1;
    public static final int ANSWER_PLAYER = 2;

    public enum Entry {
        SHARED_ADDRESS("shared_address", "(I)J", Flavour.CRITICAL),
        REPORT("report", "(IJI)V", Flavour.NORMAL);

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
