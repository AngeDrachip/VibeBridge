package com.example.nested;

import com.example.nestedlib.Helper;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

/**
 * Integration fixture with a real dependency bundled under
 * {@code META-INF/jarjar/}. The constructor references a class that only
 * exists in the nested jar, so construction succeeds only when Vibe adds the
 * extracted nested archive to the mod class loader.
 */
@Mod("nested_demo")
public final class NestedDemoMod {
    private static final Logger LOG = LogManager.getLogger("nested_demo");

    public static volatile String helperMessage;

    public NestedDemoMod(FMLJavaModLoadingContext context) {
        helperMessage = Helper.greet();
        LOG.info(helperMessage);
    }
}
