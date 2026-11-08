package com.example.registry;

import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraftforge.eventbus.api.IEventBus;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext;
import net.minecraftforge.registries.DeferredRegister;
import net.minecraftforge.registries.ForgeRegistries;
import net.minecraftforge.registries.RegistryObject;

/**
 * Integration fixture that exercises the functional registry subset:
 * {@code DeferredRegister}, {@code RegistryObject}, items, blocks, creative
 * tabs, and the {@code RegisterEvent} lifecycle. The runtime reports the
 * materialized registries, so a passing test proves entries really exist.
 */
@Mod("registry_demo")
public final class RegistryDemoMod {
    public static final String MOD_ID = "registry_demo";

    public static final DeferredRegister<Item> ITEMS =
            DeferredRegister.create(ForgeRegistries.ITEMS, MOD_ID);
    public static final DeferredRegister<Block> BLOCKS =
            DeferredRegister.create(ForgeRegistries.BLOCKS, MOD_ID);
    public static final DeferredRegister<CreativeModeTab> TABS =
            DeferredRegister.create(ForgeRegistries.CREATIVE_MODE_TABS, MOD_ID);

    public static final RegistryObject<Item> GEM =
            ITEMS.register("gem", () -> new Item(new Item.Properties().stacksTo(16)));
    public static final RegistryObject<Block> GEM_BLOCK =
            BLOCKS.register("gem_block", () -> new Block(BlockBehaviour.Properties.of()
                    .strength(3.0f, 6.0f)
                    .requiresCorrectToolForDrops()));
    public static final RegistryObject<CreativeModeTab> TAB =
            TABS.register("registry_demo", () -> CreativeModeTab.builder()
                    .title(Component.literal("Registry Demo"))
                    .icon(() -> new ItemStack(GEM.get()))
                    .build());

    public RegistryDemoMod(FMLJavaModLoadingContext context) {
        IEventBus bus = context.getModEventBus();
        ITEMS.register(bus);
        BLOCKS.register(bus);
        TABS.register(bus);
    }

    /** Used by diagnostics to prove the item properties survived registration. */
    public static String summary() {
        ResourceLocation gemId = ResourceLocation.fromNamespaceAndPath(MOD_ID, "gem");
        return gemId + " maxStack=" + GEM.get().getProperties().getMaxStackSize();
    }
}
