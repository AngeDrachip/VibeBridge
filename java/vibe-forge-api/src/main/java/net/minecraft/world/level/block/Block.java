package net.minecraft.world.level.block;

import net.minecraft.world.level.block.state.BlockBehaviour;

/**
 * Vibe's functional subset of Minecraft's {@code Block}.
 *
 * <p>A block keeps its declared properties and is stored in a real in-memory
 * block registry. Vibe does not place or render it.
 */
public class Block {
    private final BlockBehaviour.Properties properties;

    public Block(BlockBehaviour.Properties properties) {
        this.properties = properties == null ? new BlockBehaviour.Properties() : properties;
    }

    public BlockBehaviour.Properties getProperties() {
        return properties;
    }

    @Override
    public String toString() {
        return getClass().getSimpleName() + "[hardness=" + properties.getDestroyTime() + "]";
    }

    /**
     * Older mods construct {@code new Block(Block.Properties.of()...)}. The
     * nested type keeps that source shape working while sharing one real
     * properties implementation.
     */
    public static class Properties extends BlockBehaviour.Properties {
        public static Properties of() {
            return new Properties();
        }
    }
}
