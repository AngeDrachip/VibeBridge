package net.minecraft.resources;

import java.util.Objects;

/**
 * Vibe's functional subset of Minecraft's {@code ResourceLocation}.
 *
 * <p>This is a real value type: it parses, validates, compares, and hashes
 * namespaced identifiers. It is not a no-op placeholder. Vibe does not ship a
 * Minecraft game, so a location names an entry in Vibe's in-memory registries
 * rather than a world object.
 */
public final class ResourceLocation implements Comparable<ResourceLocation> {
    public static final String DEFAULT_NAMESPACE = "minecraft";
    public static final char NAMESPACE_SEPARATOR = ':';

    private final String namespace;
    private final String path;

    public ResourceLocation(String location) {
        this(splitNamespace(location), splitPath(location));
    }

    public ResourceLocation(String namespace, String path) {
        this.namespace = requirePart(namespace, "namespace");
        this.path = requirePart(path, "path");
    }

    public static ResourceLocation parse(String location) {
        return new ResourceLocation(location);
    }

    public static ResourceLocation tryParse(String location) {
        if (location == null) {
            return null;
        }
        try {
            return new ResourceLocation(location);
        } catch (RuntimeException invalid) {
            return null;
        }
    }

    public static ResourceLocation fromNamespaceAndPath(String namespace, String path) {
        return new ResourceLocation(namespace, path);
    }

    public static ResourceLocation withDefaultNamespace(String path) {
        return new ResourceLocation(DEFAULT_NAMESPACE, path);
    }

    public String getNamespace() {
        return namespace;
    }

    public String getPath() {
        return path;
    }

    @Override
    public int compareTo(ResourceLocation other) {
        int namespaceOrder = namespace.compareTo(other.namespace);
        return namespaceOrder != 0 ? namespaceOrder : path.compareTo(other.path);
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }
        if (!(other instanceof ResourceLocation location)) {
            return false;
        }
        return namespace.equals(location.namespace) && path.equals(location.path);
    }

    @Override
    public int hashCode() {
        return Objects.hash(namespace, path);
    }

    @Override
    public String toString() {
        return namespace + NAMESPACE_SEPARATOR + path;
    }

    private static String splitNamespace(String location) {
        if (location == null) {
            throw new IllegalArgumentException("resource location is null");
        }
        int separator = location.indexOf(NAMESPACE_SEPARATOR);
        return separator < 0 ? DEFAULT_NAMESPACE : location.substring(0, separator);
    }

    private static String splitPath(String location) {
        if (location == null) {
            throw new IllegalArgumentException("resource location is null");
        }
        int separator = location.indexOf(NAMESPACE_SEPARATOR);
        return separator < 0 ? location : location.substring(separator + 1);
    }

    private static String requirePart(String value, String label) {
        if (value == null || value.isEmpty()) {
            throw new IllegalArgumentException("resource location " + label + " is empty");
        }
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            boolean allowed = character == '_' || character == '-' || character == '.'
                    || character == '/' || (character >= 'a' && character <= 'z')
                    || (character >= '0' && character <= '9');
            if (!allowed) {
                throw new IllegalArgumentException(
                        "invalid character " + character + " in resource location " + label + " " + value);
            }
        }
        return value;
    }
}
