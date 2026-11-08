package net.minecraftforge.registries;

import net.minecraft.core.registries.Registries;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;

/**
 * Vibe's stable static views of the registries it implements.
 *
 * <p>Only registries with real storage are exposed. A mod that needs another
 * Forge registry is reported as unsupported instead of receiving an empty
 * placeholder.
 */
public final class ForgeRegistries {
    public static final IForgeRegistry<Item> ITEMS = VibeRegistryManager.get(Registries.ITEM);
    public static final IForgeRegistry<Block> BLOCKS = VibeRegistryManager.get(Registries.BLOCK);
    public static final IForgeRegistry<CreativeModeTab> CREATIVE_MODE_TABS =
            VibeRegistryManager.get(Registries.CREATIVE_MODE_TAB);

    private ForgeRegistries() {
    }
}
