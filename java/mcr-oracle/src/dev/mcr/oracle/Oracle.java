package dev.mcr.oracle;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import dev.mcr.oracle.font.FontHarness;
import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.OutputStreamWriter;
import java.io.PrintStream;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.Map;
import java.util.function.Function;

/**
 * {@code Oracle <harness> [--cases FILE|-] [--out FILE|-] [--assets DIR --asset-index ID]}: answers each case line
 * with an {@code {"id", "out"}} line. Exit codes: 0 done, 2 usage or setup error.
 */
public final class Oracle {
    private static final Map<String, Function<Boot, Harness>> HARNESSES = Map.of("font", FontHarness::new);
    private static final Gson GSON = new GsonBuilder().disableHtmlEscaping().create();

    private Oracle() {
    }

    public static void main(String[] args) {
        PrintStream results = System.out;
        // Vanilla logs to stdout; results keep the real one.
        System.setOut(System.err);
        try {
            run(args, results);
        } catch (Exception e) {
            e.printStackTrace();
            System.exit(2);
        }
        // Vanilla's worker pools are not daemons.
        System.exit(0);
    }

    private static String flag(String[] args, String name) {
        for (int i = 1; i + 1 < args.length; i++) {
            if (args[i].equals(name)) {
                return args[i + 1];
            }
        }
        return null;
    }

    private static void run(String[] args, PrintStream stdout) throws Exception {
        if (args.length < 1 || !HARNESSES.containsKey(args[0])) {
            throw new IllegalArgumentException("usage: Oracle <" + String.join("|", HARNESSES.keySet())
                    + "> [--cases FILE|-] [--out FILE|-] [--assets DIR --asset-index ID]");
        }
        String cases = flag(args, "--cases");
        String out = flag(args, "--out");
        String assets = flag(args, "--assets");
        Boot boot = new Boot(assets == null ? null : Path.of(assets), flag(args, "--asset-index"));
        Harness harness = HARNESSES.get(args[0]).apply(boot);

        InputStream in = cases == null || cases.equals("-") ? System.in : new FileInputStream(cases);
        OutputStream sink = out == null || out.equals("-") ? stdout : new FileOutputStream(out);
        try (BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8));
                Writer writer = new BufferedWriter(new OutputStreamWriter(sink, StandardCharsets.UTF_8))) {
            String line;
            while ((line = reader.readLine()) != null) {
                if (line.isBlank()) {
                    continue;
                }
                JsonObject entry = JsonParser.parseString(line).getAsJsonObject();
                JsonElement result;
                try {
                    result = harness.run(entry.getAsJsonObject("case"));
                } catch (RuntimeException e) {
                    JsonObject error = new JsonObject();
                    error.addProperty("error", e.toString());
                    result = error;
                }
                JsonObject answer = new JsonObject();
                answer.add("id", entry.get("id"));
                answer.add("out", result);
                writer.write(GSON.toJson(answer));
                writer.write('\n');
            }
        }
    }
}
