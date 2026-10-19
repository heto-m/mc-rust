package dev.mcr.bridge;

import java.util.Arrays;
import java.util.Collections;
import java.util.Enumeration;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;

/** A resource pack zip's entries sorted by name, so listing a directory reads only that directory's entries. */
public final class ZipIndex {
    private static final Map<ZipFile, ZipIndex> INDEXES = Collections.synchronizedMap(new WeakHashMap<>());

    private final ZipEntry[] entries;
    private final String[] sorted;
    private final int[] order;

    private ZipIndex(ZipFile zip) {
        entries = zip.stream().toArray(ZipEntry[]::new);
        Integer[] byName = new Integer[entries.length];
        for (int i = 0; i < byName.length; i++) {
            byName[i] = i;
        }
        Arrays.sort(byName, (a, b) -> entries[a].getName().compareTo(entries[b].getName()));
        sorted = new String[entries.length];
        order = new int[entries.length];
        for (int i = 0; i < byName.length; i++) {
            order[i] = byName[i];
            sorted[i] = entries[byName[i]].getName();
        }
    }

    /** The entries of {@code zip} whose names start with {@code prefix}, in the zip's own order. */
    public static Enumeration<ZipEntry> startingWith(ZipFile zip, String prefix) {
        ZipIndex index = INDEXES.get(zip);
        if (index == null) {
            index = new ZipIndex(zip);
            INDEXES.put(zip, index);
        }
        return index.find(prefix);
    }

    private Enumeration<ZipEntry> find(String prefix) {
        int from = Arrays.binarySearch(sorted, prefix);
        if (from < 0) {
            from = -from - 1;
        }
        int to = from;
        while (to < sorted.length && sorted[to].startsWith(prefix)) {
            to++;
        }
        int[] hits = Arrays.copyOfRange(order, from, to);
        Arrays.sort(hits);
        ZipEntry[] found = new ZipEntry[hits.length];
        for (int i = 0; i < hits.length; i++) {
            found[i] = entries[hits[i]];
        }
        return Collections.enumeration(Arrays.asList(found));
    }
}
