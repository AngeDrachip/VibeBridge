package net.minecraftforge.fml.event.lifecycle;

import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.fml.event.IModBusEvent;

/**
 * Vibe subset of Forge's parallel dispatch event. Work enqueued here runs
 * immediately on the calling thread, which Vibe records as an observable
 * limitation.
 */
public class ParallelDispatchEvent extends Event implements IModBusEvent {
    public void enqueueWork(Runnable work) {
        work.run();
    }
}
