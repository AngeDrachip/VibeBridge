package com.example.hello;

import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.eventbus.api.SubscribeEvent;
import net.minecraftforge.fml.ModLoadingContext;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.event.FMLPreInitializationEvent;
import net.minecraftforge.fml.event.lifecycle.FMLClientSetupEvent;
import net.minecraftforge.fml.event.lifecycle.FMLCommonSetupEvent;
import net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

/**
 * An integration-test mod that uses the modern Forge 26.x constructor shape.
 * It exists to prove Vibe constructs entrypoints, binds
 * {@link ModLoadingContext}, registers typed and reflective listeners, and
 * invokes the lifecycle. It is not a compatibility claim for real Forge mods.
 */
@Mod("hello_forge")
public final class HelloForgeMod {
    private static final Logger LOG = LogManager.getLogger("hello_forge");

    public HelloForgeMod(FMLJavaModLoadingContext context) {
        if (!"hello_forge".equals(ModLoadingContext.get().getContainer().getModId())) {
            throw new IllegalStateException("the active mod context is not hello_forge");
        }
        context.getModEventBus().addListener(FMLCommonSetupEvent.class,
                event -> LOG.info("common setup lambda fired"));
        // Posted during construction so the record proves the event bus works
        // before the lifecycle starts.
        context.getModEventBus().post(new PingEvent());
        LOG.info("hello_forge constructed");
    }

    @Mod.EventHandler
    public void preInit(FMLPreInitializationEvent event) {
        LOG.info("hello_forge legacy preInit");
    }

    public static final class PingEvent extends Event {
    }

    @Mod.EventBusSubscriber(modid = "hello_forge", bus = Mod.Bus.MOD)
    public static final class Handlers {
        private Handlers() {
        }

        @SubscribeEvent
        public static void onCommonSetup(FMLCommonSetupEvent event) {
            LOG.info("hello_forge static common setup");
        }

        @SubscribeEvent
        public static void onClientSetup(FMLClientSetupEvent event) {
            LOG.info("hello_forge static client setup");
        }

        @SubscribeEvent
        public static void onPing(PingEvent event) {
            LOG.info("hello_forge static ping");
        }
    }
}
