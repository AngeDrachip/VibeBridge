package com.example.blocked;

import net.minecraftforge.fml.common.Mod;

@Mod("minecraft_ref")
public final class MinecraftRefMod {
    public static Object make() {
        return new net.minecraft.world.level.Level();
    }
}
