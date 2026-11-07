package com.example.blocked;

import net.minecraftforge.fml.common.Mod;

@Mod("unsupported_forge")
public final class UnsupportedForgeMod {
    public static Object make() {
        return new net.minecraftforge.registries.DeferredRegister();
    }
}
