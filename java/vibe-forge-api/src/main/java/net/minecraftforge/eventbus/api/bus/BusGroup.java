package net.minecraftforge.eventbus.api.bus;

import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.eventbus.api.IEventBus;

/**
 * Minimal BusGroup facade. Real Forge groups buses by marker interface; Vibe
 * uses one bus per mod and this facade gives source-compatible access to it.
 */
public final class BusGroup {
    public static final BusGroup DEFAULT = new BusGroup(null);

    private final IEventBus bus;

    private BusGroup(IEventBus bus) {
        this.bus = bus;
    }

    public static BusGroup of(IEventBus bus) {
        return new BusGroup(bus);
    }

    public static BusGroup create(String name, Class<? extends Event> marker) {
        return new BusGroup(null);
    }

    public <T extends Event> EventBus<T> getBus(Class<T> eventType) {
        if (bus == null) {
            throw new IllegalStateException("This BusGroup is not attached to a Vibe event bus");
        }
        return new EventBus<>(bus, eventType);
    }

    public void register(Object target) {
        if (bus != null) {
            bus.register(target);
        }
    }

    public void trim() {
        // Vibe's listener lists are small and already bounded by mod count.
    }
}
