package net.minecraftforge.eventbus.api.bus;

import net.minecraftforge.eventbus.api.Event;

public interface IModBusEventMarker {
    Class<? extends Event> marker();
}
