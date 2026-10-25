package dev.mcr.oracle;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;

/** One differential harness: answers a case with vanilla's own classes, in the probe's normalized form. */
public interface Harness {
    JsonElement run(JsonObject testCase);

    /** Rounds to a power-of-two grid so both sides print the same float. */
    static double round(double value, double step) {
        return Math.round(value / step) * step;
    }
}
