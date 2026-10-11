package dev.mcr.boot;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.ValueLayout;

final class Descriptors {
    static FunctionDescriptor parse(String descriptor) {
        int close = descriptor.indexOf(')');
        if (!descriptor.startsWith("(") || close < 0 || close != descriptor.length() - 2) {
            throw new IllegalArgumentException("bad descriptor " + descriptor);
        }
        String argChars = descriptor.substring(1, close);
        MemoryLayout[] args = new MemoryLayout[argChars.length()];
        for (int i = 0; i < args.length; i++) {
            args[i] = layout(argChars.charAt(i), descriptor);
        }
        char ret = descriptor.charAt(close + 1);
        return ret == 'V' ? FunctionDescriptor.ofVoid(args) : FunctionDescriptor.of(layout(ret, descriptor), args);
    }

    private static MemoryLayout layout(char c, String descriptor) {
        return switch (c) {
            case 'I' -> ValueLayout.JAVA_INT;
            case 'J' -> ValueLayout.JAVA_LONG;
            default -> throw new IllegalArgumentException("unsupported type " + c + " in " + descriptor);
        };
    }

    static String camelCase(String symbol) {
        StringBuilder out = new StringBuilder(symbol.length());
        boolean upper = false;
        for (char c : symbol.toCharArray()) {
            if (c == '_') {
                upper = true;
            } else {
                out.append(upper ? Character.toUpperCase(c) : c);
                upper = false;
            }
        }
        return out.toString();
    }

    private Descriptors() {}
}
