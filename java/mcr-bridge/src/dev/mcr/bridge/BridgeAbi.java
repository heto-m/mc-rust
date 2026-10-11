package dev.mcr.bridge;

import dev.mcr.boot.Abi.Flavour;

/** Mirror of mcr-modbridge's extension table and messages. Entries are append-only; Rust tests compare both. */
public final class BridgeAbi {
    public static final int VERSION = 1;
    public static final String TABLE_PROPERTY = "mcr.bridge.table";

    public static final int SHARED_CONTROL_RING = 0;
    public static final int SHARED_CONTROL_BELL = 1;

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
