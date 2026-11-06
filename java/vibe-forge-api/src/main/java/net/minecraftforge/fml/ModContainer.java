package net.minecraftforge.fml;

/**
 * Vibe's minimal ModContainer surface. Forge's container carries a full
 * {@code IModInfo}; Vibe exposes only the identity methods its subset uses.
 */
public abstract class ModContainer {
    private final String modId;
    private final String version;
    private Object mod;

    protected ModContainer(String modId, String version, Object mod) {
        this.modId = modId;
        this.version = version;
        this.mod = mod;
    }

    protected void setMod(Object mod) {
        this.mod = mod;
    }

    public String getModId() {
        return modId;
    }

    public String getVersion() {
        return version;
    }

    public Object getMod() {
        return mod;
    }

    @Override
    public String toString() {
        return "ModContainer[" + modId + "]";
    }
}
