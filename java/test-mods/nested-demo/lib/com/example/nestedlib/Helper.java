package com.example.nestedlib;

/** Bundled dependency used to prove nested-JAR class loading works. */
public final class Helper {
    private Helper() {
    }

    public static String greet() {
        return "nested library online";
    }
}
