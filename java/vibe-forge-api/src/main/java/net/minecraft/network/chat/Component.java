package net.minecraft.network.chat;

/**
 * Vibe's functional subset of Minecraft text components.
 *
 * <p>Components hold the text a mod supplied. Vibe has no translation table, so
 * {@link #getString()} returns the literal text or the translation key. This is
 * deliberate: silently inventing translated strings would hide missing data.
 */
public interface Component {
    String getString();

    static Component literal(String text) {
        String value = text == null ? "" : text;
        return () -> value;
    }

    static Component translatable(String key, Object... arguments) {
        String value = key == null ? "" : key;
        return () -> value;
    }

    static Component empty() {
        return literal("");
    }
}
