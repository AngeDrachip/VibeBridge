package net.minecraftforge.registries;

import java.util.function.Supplier;

import net.minecraft.resources.ResourceLocation;

/**
 * A deferred registry entry. The value is created lazily by its supplier and
 * then stored in the owning registry when registration runs.
 */
public final class RegistryObject<T> implements Supplier<T> {
    private final ResourceLocation id;
    private final Supplier<? extends T> supplier;
    private T value;

    RegistryObject(ResourceLocation id, Supplier<? extends T> supplier) {
        if (supplier == null) {
            throw new IllegalArgumentException("registry supplier for " + id + " is null");
        }
        this.id = id;
        this.supplier = supplier;
    }

    @Override
    public T get() {
        if (value == null) {
            value = supplier.get();
        }
        return value;
    }

    public ResourceLocation getId() {
        return id;
    }

    public boolean isPresent() {
        return true;
    }

    @Override
    public String toString() {
        return "RegistryObject[" + id + "]";
    }
}
