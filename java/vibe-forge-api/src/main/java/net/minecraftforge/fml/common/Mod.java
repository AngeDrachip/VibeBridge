package net.minecraftforge.fml.common;

import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Vibe's clean-room subset of Forge's {@code @Mod} entrypoint annotation.
 *
 * <p>This is not Forge code. It intentionally matches the public descriptor and
 * the element names used by javafml mods so that unmodified jars can link
 * against the subset Vibe implements. Elements that Vibe does not interpret are
 * present only for source/annotation compatibility.
 */
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.TYPE)
public @interface Mod {
    String value() default "";

    String modid() default "";

    String name() default "";

    String version() default "";

    @Retention(RetentionPolicy.RUNTIME)
    @Target(ElementType.TYPE)
    @interface EventBusSubscriber {
        Bus bus() default Bus.FORGE;

        String modid() default "";

        Dist[] value() default {};
    }

    @Retention(RetentionPolicy.RUNTIME)
    @Target(ElementType.METHOD)
    @interface EventHandler {
    }

    enum Bus {
        FORGE,
        MOD
    }

    enum Dist {
        CLIENT,
        DEDICATED_SERVER
    }
}
