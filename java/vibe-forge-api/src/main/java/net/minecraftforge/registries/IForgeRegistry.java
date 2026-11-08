package net.minecraftforge.registries;

import java.util.Collection;

import net.minecraft.core.Registry;
import net.minecraft.resources.ResourceLocation;

/**
 * Vibe's subset of Forge's {@code IForgeRegistry}.
 *
 * <p>It is backed by {@link VibeRegistry}, a real ordered map. It is not a
 * placeholder: entries registered here are visible to mods and reported by the
 * runtime.
 */
public interface IForgeRegistry<T> extends Registry<T> {
    T getValue(ResourceLocation key);

    ResourceLocation getRegistryName();

    Collection<T> getValues();
}
