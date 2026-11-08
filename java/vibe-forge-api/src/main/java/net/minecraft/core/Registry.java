package net.minecraft.core;

import java.util.Set;

import net.minecraft.resources.ResourceLocation;

/**
 * Vibe's read-only view of a Minecraft registry.
 *
 * <p>Registries are backed by real ordered maps in the Vibe runtime. Reading a
 * missing key returns {@code null}, matching Minecraft's lookup behaviour.
 */
public interface Registry<T> {
    ResourceLocation getKey(T value);

    T get(ResourceLocation key);

    boolean containsKey(ResourceLocation key);

    Set<ResourceLocation> keySet();

    int size();
}
