package net.minecraftforge.registries;

import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Set;
import java.util.function.Supplier;

import net.minecraft.core.Registry;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraftforge.eventbus.api.IEventBus;

/**
 * Vibe's functional subset of Forge's {@code DeferredRegister}.
 *
 * <p>Entries are created lazily and inserted into a real in-memory registry.
 * Registration is idempotent so posting {@code RegisterEvent} and the runtime's
 * fallback materialization cannot create duplicates.
 */
public final class DeferredRegister<T> {
    private final ResourceLocation registryName;
    private final String modid;
    private final List<RegistryObject<T>> entries = new ArrayList<>();
    private final Set<RegistryObject<T>> materialized =
            Collections.newSetFromMap(new IdentityHashMap<>());

    private DeferredRegister(ResourceLocation registryName, String modid) {
        this.registryName = registryName;
        this.modid = modid;
    }

    public static <T> DeferredRegister<T> create(ResourceKey<Registry<T>> registry, String modid) {
        DeferredRegister<T> deferred = new DeferredRegister<>(registry.location(), modid);
        VibeRegistryManager.track(deferred);
        return deferred;
    }

    public static <T> DeferredRegister<T> create(IForgeRegistry<T> registry, String modid) {
        DeferredRegister<T> deferred = new DeferredRegister<>(registry.getRegistryName(), modid);
        VibeRegistryManager.track(deferred);
        return deferred;
    }

    public RegistryObject<T> register(String name, Supplier<? extends T> supplier) {
        ResourceLocation id = ResourceLocation.fromNamespaceAndPath(modid, name);
        RegistryObject<T> object = new RegistryObject<>(id, supplier);
        entries.add(object);
        return object;
    }

    /**
     * Subscribe to {@code RegisterEvent}. Modern mods call this from their
     * constructor; the runtime also materializes leftovers so a mod that only
     * declares a register is not silently dropped.
     */
    public void register(IEventBus bus) {
        bus.addListener(RegisterEvent.class, event -> {
            if (registryName.equals(event.getRegistryName())) {
                materialize();
            }
        });
    }

    public List<RegistryObject<T>> getEntries() {
        return List.copyOf(entries);
    }

    public ResourceLocation getRegistryName() {
        return registryName;
    }

    @SuppressWarnings("unchecked")
    synchronized void materialize() {
        VibeRegistry<T> registry = (VibeRegistry<T>) VibeRegistryManager.get(registryName);
        for (RegistryObject<T> entry : entries) {
            if (materialized.add(entry)) {
                registry.register(entry.getId(), entry.get());
            }
        }
    }
}
