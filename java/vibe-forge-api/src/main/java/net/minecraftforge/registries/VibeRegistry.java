package net.minecraftforge.registries;

import java.util.Collection;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;

import net.minecraft.resources.ResourceLocation;

/**
 * A real, ordered, in-memory registry used by Vibe's compatibility subset.
 *
 * <p>Duplicate keys are rejected rather than silently replacing an entry. That
 * makes a bad registration visible in the run report instead of hiding it.
 */
public final class VibeRegistry<T> implements IForgeRegistry<T> {
    private final ResourceLocation name;
    private final Map<ResourceLocation, T> entries = new LinkedHashMap<>();
    private final Map<T, ResourceLocation> keys = new IdentityHashMap<>();

    VibeRegistry(ResourceLocation name) {
        this.name = name;
    }

    public T register(ResourceLocation key, T value) {
        if (key == null) {
            throw new IllegalArgumentException("registry key is null");
        }
        if (value == null) {
            throw new IllegalArgumentException("registry value for " + key + " is null");
        }
        if (entries.containsKey(key)) {
            throw new IllegalStateException("duplicate " + name + " entry " + key);
        }
        entries.put(key, value);
        keys.put(value, key);
        return value;
    }

    @Override
    public ResourceLocation getKey(T value) {
        return keys.get(value);
    }

    @Override
    public T get(ResourceLocation key) {
        return entries.get(key);
    }

    @Override
    public T getValue(ResourceLocation key) {
        return entries.get(key);
    }

    @Override
    public boolean containsKey(ResourceLocation key) {
        return entries.containsKey(key);
    }

    @Override
    public Set<ResourceLocation> keySet() {
        return Collections.unmodifiableSet(entries.keySet());
    }

    @Override
    public int size() {
        return entries.size();
    }

    @Override
    public ResourceLocation getRegistryName() {
        return name;
    }

    @Override
    public Collection<T> getValues() {
        return Collections.unmodifiableCollection(entries.values());
    }

    public Map<ResourceLocation, T> entryMap() {
        return Collections.unmodifiableMap(entries);
    }

    void clear() {
        entries.clear();
        keys.clear();
    }
}
