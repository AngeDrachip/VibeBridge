package com.example.blocked;

import net.minecraftforge.fml.common.Mod;

@Mod("unsupported_forge")
public final class UnsupportedForgeMod {
    public static Object make() {
        return net.minecraftforge.common.capabilities.Capability.class;
    }
}
