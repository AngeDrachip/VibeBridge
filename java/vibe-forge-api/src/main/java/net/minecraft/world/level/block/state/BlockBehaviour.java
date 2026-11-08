package net.minecraft.world.level.block.state;

/**
 * Vibe's functional subset of Minecraft's {@code BlockBehaviour}.
 *
 * <p>Block properties are real data: hardness, resistance, and collision flags
 * are stored and can be read back. Vibe has no world, so it does not place or
 * tick blocks.
 */
public class BlockBehaviour {
    public static class Properties {
        private float destroyTime = 1.0f;
        private float explosionResistance = 1.0f;
        private boolean requiresCorrectToolForDrops;
        private boolean hasCollision = true;
        private boolean occlusion = true;

        public static Properties of() {
            return new Properties();
        }

        public Properties strength(float destroyTime, float explosionResistance) {
            this.destroyTime = destroyTime;
            this.explosionResistance = explosionResistance;
            return this;
        }

        public Properties strength(float strength) {
            return strength(strength, strength);
        }

        public Properties requiresCorrectToolForDrops() {
            this.requiresCorrectToolForDrops = true;
            return this;
        }

        public Properties noCollission() {
            this.hasCollision = false;
            return this;
        }

        public Properties noOcclusion() {
            this.occlusion = false;
            return this;
        }

        public Properties instabreak() {
            return strength(0.0f);
        }

        public float getDestroyTime() {
            return destroyTime;
        }

        public float getExplosionResistance() {
            return explosionResistance;
        }

        public boolean isRequiresCorrectToolForDrops() {
            return requiresCorrectToolForDrops;
        }

        public boolean hasCollision() {
            return hasCollision;
        }

        public boolean hasOcclusion() {
            return occlusion;
        }
    }
}
