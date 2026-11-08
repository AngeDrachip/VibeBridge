package net.minecraftforge.registries;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import net.minecraft.core.Registry;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;

/**
 * Owns the in-memory registries for one Vibe runtime session.
 *
 * <p>Registries are stable objects so {@code ForgeRegistries.ITEMS} and other
 * static views keep pointing at the live registry across reloads. {@link #reset()}
 * clears their entries instead of replacing them.
 */
public final class VibeRegistryManager {
    private static final Map<ResourceLocation, VibeRegistry<?>> REGISTRIES = new LinkedHashMap<>();
    private static final List<DeferredRegister<?>> DEFERRED = new ArrayList<>();

    private VibeRegistryManager() {
    }

    public static synchronized <T> VibeRegistry<T> get(ResourceLocation name) {
        @SuppressWarnings("unchecked")
        VibeRegistry<T> existing = (VibeRegistry<T>) REGISTRIES.get(name);
        if (existing != null) {
            return existing;
        }
        VibeRegistry<T> created = new VibeRegistry<>(name);
        REGISTRIES.put(name, created);
        return created;
    }

    public static synchronized <T> VibeRegistry<T> get(ResourceKey<Registry<T>> key) {
        return get(key.location());
    }

    static synchronized void track(DeferredRegister<?> deferred) {
        DEFERRED.add(deferred);
    }

    /** Materialize every deferred entry that has not been registered yet. */
    public static synchronized void materializeAll() {
        for (DeferredRegister<?> deferred : DEFERRED) {
            deferred.materialize();
        }
    }

    /** Clear all registry contents and forget deferred registers. */
    public static synchronized void reset() {
        for (VibeRegistry<?> registry : REGISTRIES.values()) {
            registry.clear();
        }
        DEFERRED.clear();
    }

    /** A JSON-friendly snapshot of every non-empty registry. */
    public static synchronized List<Map<String, Object>> snapshot() {
        List<Map<String, Object>> result = new ArrayList<>();
        for (VibeRegistry<?> registry : REGISTRIES.values()) {
            Map<String, Object> entry = new LinkedHashMap<>();
            entry.put("registry", registry.getRegistryName().toString());
            entry.put("size", registry.size());
            List<String> ids = new ArrayList<>();
            for (ResourceLocation key : registry.keySet()) {
                ids.add(key.toString());
            }
            entry.put("entries", ids);
            result.add(entry);
        }
        return result;
    }
}
