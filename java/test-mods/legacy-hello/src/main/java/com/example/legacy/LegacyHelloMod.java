package com.example.legacy;

import net.minecraftforge.fml.ModList;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.event.FMLConstructionEvent;
import net.minecraftforge.fml.common.event.FMLInitializationEvent;
import net.minecraftforge.fml.common.event.FMLLoadCompleteEvent;
import net.minecraftforge.fml.common.event.FMLPostInitializationEvent;
import net.minecraftforge.fml.common.event.FMLPreInitializationEvent;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

/**
 * An integration-test mod using the pre-26.x metadata and no-arg constructor
 * shape. It proves Vibe also invokes {@code @Mod.EventHandler} methods and
 * exposes other loaded mods through {@code ModList}.
 */
@Mod(modid = "legacy_hello", version = "1.0.0")
public final class LegacyHelloMod {
    private static final Logger LOG = LogManager.getLogger("legacy_hello");

    public LegacyHelloMod() {
        LOG.info("legacy_hello constructed");
    }

    @Mod.EventHandler
    public void construction(FMLConstructionEvent event) {
        LOG.info("legacy_hello construction");
    }

    @Mod.EventHandler
    public void preInit(FMLPreInitializationEvent event) {
        LOG.info("legacy_hello preInit");
    }

    @Mod.EventHandler
    public void init(FMLInitializationEvent event) {
        if (!ModList.get().isLoaded("hello_forge")) {
            throw new IllegalStateException("hello_forge should be visible from ModList");
        }
        LOG.info("legacy_hello init; ModList sees hello_forge");
    }

    @Mod.EventHandler
    public void postInit(FMLPostInitializationEvent event) {
        LOG.info("legacy_hello postInit");
    }

    @Mod.EventHandler
    public void loadComplete(FMLLoadCompleteEvent event) {
        LOG.info("legacy_hello loadComplete");
    }
}
