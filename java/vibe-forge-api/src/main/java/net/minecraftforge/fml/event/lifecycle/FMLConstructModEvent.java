package net.minecraftforge.fml.event.lifecycle;

import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.fml.event.IModBusEvent;

public final class FMLConstructModEvent extends Event implements IModBusEvent {
    public FMLConstructModEvent() {
    }

    public FMLConstructModEvent(Object container, Object stage) {
    }
}
