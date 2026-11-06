package net.minecraftforge.fml;

import net.minecraftforge.eventbus.api.IEventBus;

/**
 * Process-wide active-mod context used by Forge mods that call
 * {@code ModLoadingContext.get()}. Vibe binds and clears this while invoking a
 * specific mod.
 */
public class ModLoadingContext {
    private static final ThreadLocal<ModLoadingContext> ACTIVE = new ThreadLocal<>();

    private final ModContainer container;
    private final IEventBus modEventBus;

    protected ModLoadingContext(ModContainer container, IEventBus modEventBus) {
        this.container = container;
        this.modEventBus = modEventBus;
    }

    public static ModLoadingContext get() {
        ModLoadingContext context = ACTIVE.get();
        if (context == null) {
            throw new IllegalStateException("No active mod loading context on this thread");
        }
        return context;
    }

    public static void setActive(ModLoadingContext context) {
        ACTIVE.set(context);
    }

    public static void clearActive() {
        ACTIVE.remove();
    }

    public ModContainer getContainer() {
        return container;
    }

    public IEventBus getModEventBus() {
        return modEventBus;
    }
}
