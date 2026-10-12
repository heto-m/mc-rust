package dev.mcr.bridge;

import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.LinkedHashSet;
import java.util.Set;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.core.LogEvent;
import org.apache.logging.log4j.core.LoggerContext;
import org.apache.logging.log4j.core.config.Configuration;
import org.apache.logging.log4j.core.config.LoggerConfig;
import org.apache.logging.log4j.core.filter.AbstractFilter;
import org.apache.logging.log4j.core.impl.Log4jContextFactory;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Requests to hosts outside mcr's web allow-list fail with the proxy's 403. Mods report that as errors with stack
 * traces (Mod Menu, IPN update checks); this filter replaces them with one line per logger.
 */
final class QuietRefusals extends AbstractFilter {
    private static final Logger LOGGER = LoggerFactory.getLogger("mcr-proxy");
    private static final QuietRefusals INSTANCE = new QuietRefusals();
    private static final int MAX_CAUSES = 8;
    private static final int MAX_NOTED = 64;
    private static final String[] REFUSALS = {"Tunnel failed, got: 403", "Proxy returns \"HTTP/1.1 403"};

    private static final Set<LoggerConfig> installed = Collections.newSetFromMap(new IdentityHashMap<>());
    private static final Set<String> noted = new LinkedHashSet<>();

    private QuietRefusals() {}

    /** Adds the filter to every logger config of every log4j context; safe to call again. */
    static synchronized void install() {
        try {
            if (LogManager.getFactory() instanceof Log4jContextFactory factory) {
                for (LoggerContext context : factory.getSelector().getLoggerContexts()) {
                    install(context);
                }
            }
            if (LogManager.getContext(false) instanceof LoggerContext context) {
                install(context);
            }
        } catch (RuntimeException | LinkageError e) {
            LOGGER.warn("mcr: proxy refusals stay verbose ({})", e.toString());
        }
    }

    private static void install(LoggerContext context) {
        Configuration config = context.getConfiguration();
        boolean changed = add(config.getRootLogger());
        for (LoggerConfig logger : config.getLoggers().values()) {
            changed |= add(logger);
        }
        if (changed) {
            context.updateLoggers();
        }
    }

    private static boolean add(LoggerConfig logger) {
        if (!installed.add(logger)) {
            return false;
        }
        logger.addFilter(INSTANCE);
        return true;
    }

    @Override
    public Result filter(LogEvent event) {
        if (!refused(event)) {
            return Result.NEUTRAL;
        }
        note(event.getLoggerName());
        return Result.DENY;
    }

    private static boolean refused(LogEvent event) {
        if (event.getMessage() != null && mentionsRefusal(event.getMessage().getFormattedMessage())) {
            return true;
        }
        Throwable t = event.getThrown();
        for (int i = 0; t != null && i < MAX_CAUSES; i++, t = t.getCause()) {
            if (mentionsRefusal(t.getMessage())) {
                return true;
            }
        }
        return false;
    }

    private static boolean mentionsRefusal(String text) {
        if (text == null) {
            return false;
        }
        for (String refusal : REFUSALS) {
            if (text.contains(refusal)) {
                return true;
            }
        }
        return false;
    }

    private static void note(String logger) {
        String name = logger == null || logger.isEmpty() ? "?" : logger;
        synchronized (noted) {
            if (noted.size() >= MAX_NOTED || !noted.add(name)) {
                return;
            }
        }
        LOGGER.info("{}: a web request was refused, its host is not on mcr's allow-list (http_hosts in mod-policy.json)", name);
    }
}
