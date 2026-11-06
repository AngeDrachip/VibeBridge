package dev.vibe.runtime;

import java.lang.annotation.Annotation;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.Consumer;

import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.eventbus.api.EventPriority;
import net.minecraftforge.eventbus.api.IEventBus;

/**
 * Vibe's event bus. It supports reflective {@code @SubscribeEvent} listeners and
 * explicit consumer listeners, with Forge-style priorities and cancellation
 * filtering. It records every dispatch for diagnostics and integration tests.
 */
public final class VibeEventBus implements IEventBus {
    private static final int MAX_RECORDS = 5000;

    private final String name;
    private final CopyOnWriteArrayList<Handler> handlers = new CopyOnWriteArrayList<>();
    private final List<Map<String, Object>> records = new ArrayList<>();
    private int lambdaCounter;

    public VibeEventBus(String name) {
        this.name = name;
    }

    public String name() {
        return name;
    }

    @Override
    public void register(Object target) {
        if (target == null) {
            return;
        }
        Class<?> type = target instanceof Class<?> clazz ? clazz : target.getClass();
        for (Class<?> current = type; current != null && current != Object.class; current = current.getSuperclass()) {
            for (Method method : current.getDeclaredMethods()) {
                Annotation annotation = subscribeEventAnnotation(method);
                if (annotation == null) {
                    continue;
                }
                if (method.getParameterCount() != 1 || !Event.class.isAssignableFrom(method.getParameterTypes()[0])) {
                    record("(registration)", type.getName() + "#" + method.getName(), false,
                            "listener must take exactly one Event parameter");
                    continue;
                }
                if (!Modifier.isPublic(method.getModifiers())) {
                    try {
                        method.setAccessible(true);
                    } catch (RuntimeException ignored) {
                        // fall through; invoke will report the failure
                    }
                }
                boolean receiveCanceled = booleanAttribute(annotation, "receiveCanceled");
                handlers.add(new Handler(
                        target,
                        method,
                        method.getParameterTypes()[0],
                        priorityAttribute(annotation),
                        receiveCanceled,
                        null,
                        type.getSimpleName() + "#" + method.getName()));
            }
        }
    }

    @Override
    public void unregister(Object target) {
        handlers.removeIf(handler -> handler.target == target);
    }

    @Override
    public <T extends Event> void addListener(Consumer<T> consumer) {
        addListener(EventPriority.NORMAL, consumer);
    }

    @Override
    public <T extends Event> void addListener(EventPriority priority, Consumer<T> consumer) {
        handlers.add(Handler.forConsumer(
                Event.class,
                priority == null ? EventPriority.NORMAL : priority,
                true,
                castConsumer(consumer),
                "lambda#" + (++lambdaCounter)));
    }

    @Override
    public <T extends Event> void addListener(Class<T> eventType, Consumer<T> consumer) {
        addListener(EventPriority.NORMAL, eventType, consumer);
    }

    @Override
    public <T extends Event> void addListener(EventPriority priority, Class<T> eventType, Consumer<T> consumer) {
        addListener(priority, false, eventType, consumer);
    }

    @Override
    public <T extends Event> void addListener(EventPriority priority, boolean receiveCanceled, Class<T> eventType,
            Consumer<T> consumer) {
        handlers.add(Handler.forConsumer(
                eventType,
                priority == null ? EventPriority.NORMAL : priority,
                receiveCanceled,
                castConsumer(consumer),
                "lambda#" + (++lambdaCounter)));
    }

    @Override
    public <T extends Event> T post(T event) {
        if (event == null) {
            return null;
        }
        List<Handler> snapshot = new ArrayList<>(handlers);
        snapshot.sort(Comparator.comparingInt(handler -> handler.priority.ordinal()));
        for (Handler handler : snapshot) {
            if (!handler.eventType.isAssignableFrom(event.getClass())) {
                continue;
            }
            if (event.isCanceled() && !handler.receiveCanceled) {
                continue;
            }
            try {
                handler.invoke(event);
                record(event, handler.label, true, null);
            } catch (ClassCastException mismatch) {
                // Untyped Consumer listeners (lambdas) receive every event and
                // rely on the synthetic bridge cast. A mismatch is not an error.
                if (handler.consumer == null) {
                    record(event, handler.label, false, mismatch.toString());
                }
            } catch (Throwable throwable) {
                Throwable cause = unwrap(throwable);
                record(event, handler.label, false, cause.toString());
            }
        }
        return event;
    }

    public List<Map<String, Object>> records() {
        synchronized (records) {
            return new ArrayList<>(records);
        }
    }

    /** Record a lifecycle invocation that Vibe performed outside the bus. */
    public void recordExternal(String event, String listener, boolean ok, String error) {
        record(event, listener, ok, error);
    }

    public int handlerCount() {
        return handlers.size();
    }

    private void record(Event event, String listener, boolean ok, String error) {
        Map<String, Object> record = new LinkedHashMap<>();
        record.put("event", event.getClass().getSimpleName());
        record.put("listener", listener);
        record.put("ok", ok);
        if (error != null) {
            record.put("error", error);
        }
        synchronized (records) {
            if (records.size() >= MAX_RECORDS) {
                records.remove(0);
            }
            records.add(record);
        }
    }

    private void record(String event, String listener, boolean ok, String error) {
        Map<String, Object> record = new LinkedHashMap<>();
        record.put("event", event);
        record.put("listener", listener);
        record.put("ok", ok);
        if (error != null) {
            record.put("error", error);
        }
        synchronized (records) {
            if (records.size() >= MAX_RECORDS) {
                records.remove(0);
            }
            records.add(record);
        }
    }

    private static Throwable unwrap(Throwable throwable) {
        if (throwable instanceof InvocationTargetException invocation && invocation.getCause() != null) {
            return invocation.getCause();
        }
        return throwable;
    }

    private static Annotation subscribeEventAnnotation(Method method) {
        for (Annotation annotation : method.getAnnotations()) {
            String name = annotation.annotationType().getName();
            if (name.equals("net.minecraftforge.eventbus.api.SubscribeEvent")
                    || name.equals("net.minecraftforge.eventbus.api.listener.SubscribeEvent")) {
                return annotation;
            }
        }
        return null;
    }

    private static boolean booleanAttribute(Annotation annotation, String name) {
        try {
            Object value = annotation.annotationType().getMethod(name).invoke(annotation);
            return value instanceof Boolean bool && bool;
        } catch (ReflectiveOperationException ignored) {
            return false;
        }
    }

    private static EventPriority priorityAttribute(Annotation annotation) {
        try {
            Object value = annotation.annotationType().getMethod("priority").invoke(annotation);
            return value instanceof EventPriority priority ? priority : EventPriority.NORMAL;
        } catch (ReflectiveOperationException ignored) {
            return EventPriority.NORMAL;
        }
    }

    @SuppressWarnings("unchecked")
    private static Consumer<Event> castConsumer(Consumer<? extends Event> consumer) {
        return (Consumer<Event>) consumer;
    }

    private static final class Handler {
        private final Object target;
        private final Method method;
        private final Class<?> eventType;
        private final EventPriority priority;
        private final boolean receiveCanceled;
        private final Consumer<Event> consumer;
        private final String label;

        private Handler(Object target, Method method, Class<?> eventType, EventPriority priority,
                boolean receiveCanceled, Consumer<Event> consumer, String label) {
            this.target = target;
            this.method = method;
            this.eventType = eventType;
            this.priority = priority;
            this.receiveCanceled = receiveCanceled;
            this.consumer = consumer;
            this.label = label;
        }

        private static Handler forConsumer(Class<?> eventType, EventPriority priority, boolean receiveCanceled,
                Consumer<Event> consumer, String label) {
            return new Handler(null, null, eventType, priority, receiveCanceled, consumer, label);
        }

        private void invoke(Event event) throws Throwable {
            if (consumer != null) {
                consumer.accept(event);
            } else {
                Object receiver = Modifier.isStatic(method.getModifiers()) ? null : target;
                method.invoke(receiver, event);
            }
        }
    }
}
