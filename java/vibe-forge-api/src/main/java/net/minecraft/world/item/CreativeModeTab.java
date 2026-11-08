package net.minecraft.world.item;

import java.util.function.Supplier;

import net.minecraft.network.chat.Component;

/**
 * Vibe's functional subset of Minecraft's {@code CreativeModeTab}.
 *
 * <p>A tab keeps the title and icon a mod declared. Vibe reports the tab in its
 * registry snapshot; it cannot render a creative inventory.
 */
public class CreativeModeTab {
    private final Component title;
    private final Supplier<ItemStack> icon;

    private CreativeModeTab(Component title, Supplier<ItemStack> icon) {
        this.title = title == null ? Component.empty() : title;
        this.icon = icon == null ? () -> ItemStack.EMPTY : icon;
    }

    public Component getDisplayName() {
        return title;
    }

    public ItemStack getIconItem() {
        ItemStack stack = icon.get();
        return stack == null ? ItemStack.EMPTY : stack;
    }

    public static Builder builder() {
        return new Builder();
    }

    public static class Builder {
        private Component title = Component.empty();
        private Supplier<ItemStack> icon = () -> ItemStack.EMPTY;

        public Builder title(Component title) {
            this.title = title;
            return this;
        }

        public Builder icon(Supplier<ItemStack> icon) {
            this.icon = icon;
            return this;
        }

        public CreativeModeTab build() {
            return new CreativeModeTab(title, icon);
        }
    }
}
