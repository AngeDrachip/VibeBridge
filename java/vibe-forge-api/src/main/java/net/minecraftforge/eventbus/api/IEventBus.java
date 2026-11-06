package net.minecraftforge.eventbus.api;

import java.util.function.Consumer;

/**
 * Vibe's subset of Forge's event bus interface.
 */
public interface IEventBus {
    void register(Object target);

    void unregister(Object target);

    <T extends Event> void addListener(Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, Consumer<T> consumer);

    <T extends Event> void addListener(Class<T> eventType, Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, Class<T> eventType, Consumer<T> consumer);

    <T extends Event> void addListener(EventPriority priority, boolean receiveCanceled, Class<T> eventType, Consumer<T> consumer);

    <T extends Event> T post(T event);
}
