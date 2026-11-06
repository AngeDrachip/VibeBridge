package net.minecraftforge.fml;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * Vibe's minimal ModList facade. Populated by the Vibe runtime after mods are
 * constructed.
 */
public final class ModList {
    private static volatile ModList instance = new ModList(Collections.emptyMap());

    private final Map<String, ModContainer> mods;

    private ModList(Map<String, ModContainer> mods) {
        this.mods = mods;
    }

    public static ModList get() {
        return instance;
    }

    public static void setLoadedMods(List<? extends ModContainer> containers) {
        Map<String, ModContainer> map = new LinkedHashMap<>();
        for (ModContainer container : containers) {
            map.put(container.getModId(), container);
        }
        instance = new ModList(Collections.unmodifiableMap(map));
    }

    public boolean isLoaded(String modId) {
        return mods.containsKey(modId);
    }

    public int size() {
        return mods.size();
    }

    public List<String> getModIds() {
        return List.copyOf(mods.keySet());
    }

    public Optional<ModContainer> getModContainerById(String modId) {
        return Optional.ofNullable(mods.get(modId));
    }
}
