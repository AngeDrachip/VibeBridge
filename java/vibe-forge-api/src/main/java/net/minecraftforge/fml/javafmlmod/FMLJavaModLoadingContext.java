package net.minecraftforge.fml.javafmlmod;

import net.minecraftforge.eventbus.api.IEventBus;
import net.minecraftforge.eventbus.api.bus.BusGroup;
import net.minecraftforge.fml.ModContainer;
import net.minecraftforge.fml.ModLoadingContext;

/**
 * Vibe's compatibility implementation of Forge's javafml context.
 *
 * <p>Forge 26.3 passes this object to mod constructors. Older Forge versions
 * pass the {@link IEventBus} directly; Vibe supports both shapes.
 */
public class FMLJavaModLoadingContext extends ModLoadingContext {
    public FMLJavaModLoadingContext(ModContainer container, IEventBus modEventBus) {
        super(container, modEventBus);
    }

    public static FMLJavaModLoadingContext get() {
        ModLoadingContext context = ModLoadingContext.get();
        if (context instanceof FMLJavaModLoadingContext fml) {
            return fml;
        }
        throw new IllegalStateException("The active mod loading context is not a javafml context");
    }

    public BusGroup getModBusGroup() {
        return BusGroup.of(getModEventBus());
    }
}
