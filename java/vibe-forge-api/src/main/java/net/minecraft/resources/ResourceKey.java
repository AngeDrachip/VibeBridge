package net.minecraft.resources;

import net.minecraft.core.Registry;

/**
 * Vibe's functional subset of Minecraft's {@code ResourceKey}.
 *
 * <p>A key names one registry and one entry inside it. Deferred registration
 * uses it to route an entry to the correct in-memory registry.
 */
public final class ResourceKey<T> {
    private final ResourceLocation registry;
    private final ResourceLocation location;

    private ResourceKey(ResourceLocation registry, ResourceLocation location) {
        this.registry = registry;
        this.location = location;
    }

    public static <T> ResourceKey<T> create(ResourceKey<? extends Registry<T>> registry, ResourceLocation location) {
        return new ResourceKey<>(registry.location(), location);
    }

    public static <T> ResourceKey<Registry<T>> createRegistryKey(ResourceLocation location) {
        return new ResourceKey<>(location, location);
    }

    /** The location of the registry this key belongs to. */
    public ResourceLocation registry() {
        return registry;
    }

    /** The location of the entry named by this key. */
    public ResourceLocation location() {
        return location;
    }

    @Override
    public String toString() {
        return registry + " / " + location;
    }
}
