package org.apache.logging.log4j;

/**
 * A deliberately small Log4j facade used by many Forge mods during static
 * initialisation. Vibe does not ship Log4j; this keeps common logging calls
 * working and routes them to stderr.
 */
public interface Logger {
    String getName();

    boolean isTraceEnabled();

    boolean isDebugEnabled();

    boolean isInfoEnabled();

    boolean isWarnEnabled();

    boolean isErrorEnabled();

    void trace(String message, Object... args);

    void debug(String message, Object... args);

    void info(String message, Object... args);

    void warn(String message, Object... args);

    void error(String message, Object... args);

    void trace(String message, Throwable throwable);

    void debug(String message, Throwable throwable);

    void info(String message, Throwable throwable);

    void warn(String message, Throwable throwable);

    void error(String message, Throwable throwable);
}
