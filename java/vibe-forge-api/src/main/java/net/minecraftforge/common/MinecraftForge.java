package net.minecraftforge.common;

import net.minecraftforge.eventbus.api.IEventBus;
import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.eventbus.api.EventPriority;

import java.util.function.Consumer;

/**
 * Legacy global event bus facade. Vibe binds the delegate at runtime.
 */
public final class MinecraftForge {
    private static volatile IEventBus delegate;

    private MinecraftForge() {
    }

    public static final IEventBus EVENT_BUS = new IEventBus() {
        @Override
        public void register(Object target) {
            requireDelegate().register(target);
        }

        @Override
        public void unregister(Object target) {
            requireDelegate().unregister(target);
        }

        @Override
        public <T extends Event> void addListener(Consumer<T> consumer) {
            requireDelegate().addListener(consumer);
        }

        @Override
        public <T extends Event> void addListener(EventPriority priority, Consumer<T> consumer) {
            requireDelegate().addListener(priority, consumer);
        }

        @Override
        public <T extends Event> void addListener(Class<T> eventType, Consumer<T> consumer) {
            requireDelegate().addListener(eventType, consumer);
        }

        @Override
        public <T extends Event> void addListener(EventPriority priority, Class<T> eventType, Consumer<T> consumer) {
            requireDelegate().addListener(priority, eventType, consumer);
        }

        @Override
        public <T extends Event> void addListener(EventPriority priority, boolean receiveCanceled, Class<T> eventType, Consumer<T> consumer) {
            requireDelegate().addListener(priority, receiveCanceled, eventType, consumer);
        }

        @Override
        public <T extends Event> T post(T event) {
            return requireDelegate().post(event);
        }
    };

    public static void setEventBus(IEventBus bus) {
        delegate = bus;
    }

    private static IEventBus requireDelegate() {
        IEventBus bus = delegate;
        if (bus == null) {
            throw new IllegalStateException("Vibe has not bound MinecraftForge.EVENT_BUS");
        }
        return bus;
    }
}
