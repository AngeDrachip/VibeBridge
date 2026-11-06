package net.minecraftforge.eventbus.api.bus;

import java.util.function.Consumer;

import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.eventbus.api.EventPriority;
import net.minecraftforge.eventbus.api.IEventBus;

/**
 * Minimal facade for the newer Forge event-bus API. It deliberately delegates
 * to Vibe's single internal bus rather than reproducing Forge's grouping and
 * class-filtering internals.
 */
public final class EventBus<T extends Event> {
    private final IEventBus delegate;
    private final Class<T> eventType;

    public EventBus(IEventBus delegate, Class<T> eventType) {
        this.delegate = delegate;
        this.eventType = eventType;
    }

    public void addListener(Consumer<T> listener) {
        delegate.addListener(eventType, listener);
    }

    public void addListener(EventPriority priority, Consumer<T> listener) {
        delegate.addListener(priority, eventType, listener);
    }

    public void addListener(EventPriority priority, boolean receiveCanceled, Consumer<T> listener) {
        delegate.addListener(priority, receiveCanceled, eventType, listener);
    }

    public T post(T event) {
        return delegate.post(event);
    }
}
