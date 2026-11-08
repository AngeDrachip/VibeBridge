package net.minecraft.world.item;

/**
 * Vibe's functional subset of Minecraft's {@code ItemStack}.
 *
 * <p>Stacks hold a real item reference and a mutable count. They are used by
 * mods to describe creative-tab icons and default outputs. Vibe does not
 * simulate inventories or world interaction.
 */
public class ItemStack {
    public static final ItemStack EMPTY = new ItemStack(null, 0);

    private final Item item;
    private int count;

    public ItemStack(Item item) {
        this(item, 1);
    }

    public ItemStack(Item item, int count) {
        this.item = item;
        this.count = Math.max(count, 0);
    }

    public Item getItem() {
        return item;
    }

    public int getCount() {
        return count;
    }

    public void setCount(int count) {
        this.count = Math.max(count, 0);
    }

    public boolean isEmpty() {
        return item == null || count <= 0;
    }

    public ItemStack copy() {
        return new ItemStack(item, count);
    }

    public void grow(int amount) {
        setCount(count + amount);
    }

    public void shrink(int amount) {
        setCount(count - amount);
    }

    @Override
    public String toString() {
        return isEmpty() ? "ItemStack[EMPTY]" : count + "x" + item;
    }
}
