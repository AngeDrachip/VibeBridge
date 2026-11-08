package net.minecraft.world.item;

/**
 * Vibe's functional subset of Minecraft's {@code Item}.
 *
 * <p>An item holds its declared properties. Vibe stores it in a real in-memory
 * item registry and reports it; it does not render or tick the item because
 * there is no Minecraft world. The class is not a no-op: properties are
 * preserved and can be read back by mods and by Vibe's diagnostics.
 */
public class Item {
    private final Properties properties;

    public Item(Properties properties) {
        this.properties = properties == null ? new Properties() : properties;
    }

    public Properties getProperties() {
        return properties;
    }

    @Override
    public String toString() {
        return getClass().getSimpleName() + "[maxStack=" + properties.maxStackSize + "]";
    }

    public static class Properties {
        private int maxStackSize = 64;
        private int durability;
        private boolean fireResistant;

        public Properties stacksTo(int maxStackSize) {
            if (maxStackSize < 1) {
                throw new IllegalArgumentException("max stack size must be positive");
            }
            this.maxStackSize = maxStackSize;
            return this;
        }

        public Properties durability(int durability) {
            this.durability = durability;
            this.maxStackSize = 1;
            return this;
        }

        public Properties fireResistant() {
            this.fireResistant = true;
            return this;
        }

        public int getMaxStackSize() {
            return maxStackSize;
        }

        public int getDurability() {
            return durability;
        }

        public boolean isFireResistant() {
            return fireResistant;
        }
    }
}
