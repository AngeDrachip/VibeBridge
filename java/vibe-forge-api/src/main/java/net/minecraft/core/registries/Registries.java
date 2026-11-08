package net.minecraft.core.registries;

import net.minecraft.core.Registry;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;

/**
 * Registry keys used by the Vibe API subset.
 *
 * <p>Only registries Vibe can actually store and report are present. Adding a
 * key here is a compatibility promise: the runtime must create a real registry
 * for it.
 */
public final class Registries {
    public static final ResourceKey<Registry<Item>> ITEM =
            ResourceKey.createRegistryKey(ResourceLocation.withDefaultNamespace("item"));
    public static final ResourceKey<Registry<Block>> BLOCK =
            ResourceKey.createRegistryKey(ResourceLocation.withDefaultNamespace("block"));
    public static final ResourceKey<Registry<CreativeModeTab>> CREATIVE_MODE_TAB =
            ResourceKey.createRegistryKey(ResourceLocation.withDefaultNamespace("creative_mode_tab"));

    private Registries() {
    }
}
