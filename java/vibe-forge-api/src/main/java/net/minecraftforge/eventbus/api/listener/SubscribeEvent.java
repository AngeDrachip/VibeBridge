package net.minecraftforge.eventbus.api.listener;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

import net.minecraftforge.eventbus.api.EventPriority;

/**
 * The newer event-listener marker used by Forge 26.3's event bus. Vibe reads
 * this annotation as well as the older {@code net.minecraftforge.eventbus.api.SubscribeEvent}.
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface SubscribeEvent {
    EventPriority priority() default EventPriority.NORMAL;

    boolean receiveCanceled() default false;
}
