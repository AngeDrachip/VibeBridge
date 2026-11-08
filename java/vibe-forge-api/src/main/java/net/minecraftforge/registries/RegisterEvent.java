package net.minecraftforge.registries;

import net.minecraft.resources.ResourceLocation;
import net.minecraftforge.eventbus.api.Event;

/**
 * Vibe's subset of Forge's {@code RegisterEvent}.
 *
 * <p>The runtime posts one event per registry on each mod event bus. Deferred
 * registers use it to materialize their entries. The event carries the real
 * registry name, so listeners can filter without guessing.
 */
public class RegisterEvent extends Event {
    private final ResourceLocation registryName;

    public RegisterEvent(ResourceLocation registryName) {
        this.registryName = registryName;
    }

    public ResourceLocation getRegistryName() {
        return registryName;
    }

    @FunctionalInterface
    public interface RegisterHelper<T> {
        void register(ResourceLocation key, T value);
    }
}
