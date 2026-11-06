package net.minecraftforge.fml.common;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

import net.minecraftforge.eventbus.api.IEventBus;

/**
 * Legacy Forge global event bus facade. Vibe binds it to its internal bus when
 * the runtime starts.
 */
public final class FMLCommonHandler {
    private static final Logger LOGGER = LogManager.getLogger("vibe");

    private FMLCommonHandler() {
    }

    public static FMLCommonHandler instance() {
        return Holder.INSTANCE;
    }

    public static Logger getLogger() {
        return LOGGER;
    }

    private static final class Holder {
        private static final FMLCommonHandler INSTANCE = new FMLCommonHandler();
    }
}
