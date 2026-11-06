package org.apache.logging.log4j;

/**
 * Minimal Log4j LogManager facade. See {@link Logger}.
 */
public final class LogManager {
    private LogManager() {
    }

    public static Logger getLogger() {
        return new SimpleLogger("vibe");
    }

    public static Logger getLogger(String name) {
        return new SimpleLogger(name);
    }

    public static Logger getLogger(Class<?> type) {
        return new SimpleLogger(type.getName());
    }

    private static final class SimpleLogger implements Logger {
        private final String name;

        private SimpleLogger(String name) {
            this.name = name;
        }

        @Override
        public String getName() {
            return name;
        }

        @Override
        public boolean isTraceEnabled() {
            return false;
        }

        @Override
        public boolean isDebugEnabled() {
            return false;
        }

        @Override
        public boolean isInfoEnabled() {
            return true;
        }

        @Override
        public boolean isWarnEnabled() {
            return true;
        }

        @Override
        public boolean isErrorEnabled() {
            return true;
        }

        @Override
        public void trace(String message, Object... args) {
            // trace is intentionally disabled by default
        }

        @Override
        public void debug(String message, Object... args) {
            // debug is intentionally disabled by default
        }

        @Override
        public void info(String message, Object... args) {
            emit("INFO", message, args);
        }

        @Override
        public void warn(String message, Object... args) {
            emit("WARN", message, args);
        }

        @Override
        public void error(String message, Object... args) {
            emit("ERROR", message, args);
        }

        @Override
        public void trace(String message, Throwable throwable) {
            emit("TRACE", message, throwable);
        }

        @Override
        public void debug(String message, Throwable throwable) {
            emit("DEBUG", message, throwable);
        }

        @Override
        public void info(String message, Throwable throwable) {
            emit("INFO", message, throwable);
        }

        @Override
        public void warn(String message, Throwable throwable) {
            emit("WARN", message, throwable);
        }

        @Override
        public void error(String message, Throwable throwable) {
            emit("ERROR", message, throwable);
        }

        private void emit(String level, String message, Object... args) {
            String formatted = message;
            try {
                if (args != null && args.length > 0 && message != null && message.contains("{}")) {
                    formatted = format(message, args);
                } else if (args != null && args.length == 1 && args[0] instanceof Throwable) {
                    emit(level, message, (Throwable) args[0]);
                    return;
                }
            } catch (RuntimeException ignored) {
                formatted = message;
            }
            System.err.println("[" + name + "/" + level + "] " + formatted);
        }

        private void emit(String level, String message, Throwable throwable) {
            System.err.println("[" + name + "/" + level + "] " + message);
            if (throwable != null) {
                throwable.printStackTrace(System.err);
            }
        }

        private static String format(String pattern, Object[] args) {
            StringBuilder out = new StringBuilder();
            int arg = 0;
            int start = 0;
            int index;
            while ((index = pattern.indexOf("{}", start)) >= 0) {
                out.append(pattern, start, index);
                if (arg < args.length) {
                    out.append(args[arg++]);
                } else {
                    out.append("{}");
                }
                start = index + 2;
            }
            out.append(pattern.substring(start));
            return out.toString();
        }
    }
}
