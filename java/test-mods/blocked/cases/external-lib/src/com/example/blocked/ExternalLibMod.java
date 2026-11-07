package com.example.blocked;

import net.minecraftforge.fml.common.Mod;

@Mod("external_lib")
public final class ExternalLibMod {
    public static Object make() {
        return new com.google.gson.Gson();
    }
}
