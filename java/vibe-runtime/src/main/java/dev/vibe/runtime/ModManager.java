package dev.vibe.runtime;

import java.io.File;
import java.io.InputStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Enumeration;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.jar.JarEntry;
import java.util.jar.JarFile;

import dev.vibe.runtime.json.Json;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.registries.RegisterEvent;
import net.minecraftforge.registries.VibeRegistryManager;
import net.minecraftforge.eventbus.api.Event;
import net.minecraftforge.eventbus.api.IEventBus;
import net.minecraftforge.fml.ModContainer;
import net.minecraftforge.fml.ModList;
import net.minecraftforge.fml.ModLoadingContext;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.event.FMLConstructionEvent;
import net.minecraftforge.fml.common.event.FMLInitializationEvent;
import net.minecraftforge.fml.common.event.FMLLoadCompleteEvent;
import net.minecraftforge.fml.common.event.FMLPostInitializationEvent;
import net.minecraftforge.fml.common.event.FMLPreInitializationEvent;
import net.minecraftforge.fml.event.lifecycle.FMLClientSetupEvent;
import net.minecraftforge.fml.event.lifecycle.FMLCommonSetupEvent;
import net.minecraftforge.fml.event.lifecycle.FMLConstructModEvent;
import net.minecraftforge.fml.event.lifecycle.FMLDedicatedServerSetupEvent;
import net.minecraftforge.fml.event.lifecycle.InterModEnqueueEvent;
import net.minecraftforge.fml.event.lifecycle.InterModProcessEvent;
import net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext;

/**
 * Loads Forge-shaped jars into isolated class loaders, constructs {@code @Mod}
 * entrypoints, and runs a constrained lifecycle against Vibe's API subset.
 */
public final class ModManager {
    private final List<VibeMod> mods = new ArrayList<>();
    private final Map<String, VibeMod> byId = new LinkedHashMap<>();
    private final List<LoadedJar> jars = new ArrayList<>();
    private final List<Path> tempDirs = new ArrayList<>();
    private final VibeEventBus forgeBus = new VibeEventBus("forge");
    private String side = "SERVER";
    private String profile;
    private String backend;
    private String phase = "new";
    private final List<String> failures = new ArrayList<>();

    public ModManager() {
        MinecraftForge.setEventBus(forgeBus);
    }

    public synchronized Map<String, Object> load(List<Path> paths, String requestedSide) {
        return load(paths, requestedSide, null, null);
    }

    public synchronized Map<String, Object> load(
            List<Path> paths, String requestedSide, String requestedProfile, String requestedBackend) {
        reset();
        this.side = requestedSide == null ? "SERVER" : requestedSide.toUpperCase();
        this.profile = requestedProfile;
        this.backend = requestedBackend;
        this.phase = "loaded";

        for (Path path : paths) {
            LoadedJar loaded = loadJar(path);
            jars.add(loaded);
            if (loaded.error != null) {
                failures.add(loaded.error);
            }
        }

        registerEventBusSubscribers();
        ModList.setLoadedMods(containers());

        List<Object> modList = new ArrayList<>();
        for (VibeMod mod : mods) {
            modList.add(mod.describe());
        }
        List<Object> jarList = new ArrayList<>();
        for (LoadedJar jar : jars) {
            jarList.add(jar.describe());
        }
        return Json.object(
                "ok", failures.isEmpty(),
                "side", side,
                "profile", profile,
                "backend", backend,
                "jars", jarList,
                "mods", modList,
                "failures", new ArrayList<>(failures));
    }

    public synchronized Map<String, Object> startLifecycle() {
        if (mods.isEmpty()) {
            return Json.object(
                    "ok", false,
                    "side", side,
                    "phase", "no_mods",
                    "mods", List.of(),
                    "failures", List.of("no @Mod entrypoints were discovered"),
                    "events", allRecords());
        }
        failures.clear();
        phase = "construct";

        for (VibeMod mod : mods) {
            if (!mod.isUsable()) {
                continue;
            }
            construct(mod);
        }
        ModList.setLoadedMods(containers());

        phase = "construct_events";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                post(mod, new FMLConstructModEvent());
            }
        }
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                invokeLegacy(mod, new FMLConstructionEvent());
            }
        }

        // Forge posts one RegisterEvent per registry on each mod bus. Vibe does
        // the same for the registries it really stores, then materializes any
        // deferred entry whose mod did not subscribe to the event.
        phase = "register";
        List<ResourceLocation> registryNames = List.of(
                Registries.ITEM.location(),
                Registries.BLOCK.location(),
                Registries.CREATIVE_MODE_TAB.location());
        for (VibeMod mod : mods) {
            if (mod.instance == null) {
                continue;
            }
            for (ResourceLocation registryName : registryNames) {
                post(mod, new RegisterEvent(registryName));
            }
        }
        VibeRegistryManager.materializeAll();

        phase = "common_setup";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                post(mod, new FMLCommonSetupEvent());
            }
        }
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                invokeLegacy(mod, new FMLPreInitializationEvent());
            }
        }

        phase = "init";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                invokeLegacy(mod, new FMLInitializationEvent());
            }
        }

        phase = "sided_setup";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                if ("CLIENT".equals(side)) {
                    post(mod, new FMLClientSetupEvent());
                } else {
                    post(mod, new FMLDedicatedServerSetupEvent());
                }
            }
        }

        phase = "post_init";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                invokeLegacy(mod, new FMLPostInitializationEvent());
            }
        }

        phase = "inter_mod";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                post(mod, new InterModEnqueueEvent());
                post(mod, new InterModProcessEvent());
            }
        }

        phase = "complete";
        for (VibeMod mod : mods) {
            if (mod.instance != null) {
                post(mod, new net.minecraftforge.fml.event.lifecycle.FMLLoadCompleteEvent());
                invokeLegacy(mod, new FMLLoadCompleteEvent());
                mod.status = "initialized";
            }
        }

        boolean ok = mods.stream().allMatch(mod -> mod.instance != null);
        return Json.object(
                "ok", ok,
                "side", side,
                "phase", phase,
                "mods", statusList(),
                "failures", new ArrayList<>(failures),
                "events", allRecords());
    }

    public synchronized Map<String, Object> status() {
        return Json.object(
                "ok", true,
                "side", side,
                "profile", profile,
                "backend", backend,
                "phase", phase,
                "mods", statusList(),
                "failures", new ArrayList<>(failures));
    }

    public synchronized Map<String, Object> registrySnapshot() {
        List<Object> ids = new ArrayList<>();
        Map<String, Object> namespaces = new LinkedHashMap<>();
        for (VibeMod mod : mods) {
            ids.add(mod.id);
            namespaces.put(mod.id, List.of(mod.id));
        }
        return Json.object(
                "profile", profile,
                "backend", backend,
                "mods", ids,
                "namespaces", namespaces,
                "registries", VibeRegistryManager.snapshot(),
                "event_records", allRecords());
    }

    private LoadedJar loadJar(Path path) {
        LoadedJar loaded = new LoadedJar(path);
        File file = path.toFile();
        if (!file.isFile()) {
            loaded.error = "mod jar does not exist: " + path;
            return loaded;
        }
        try {
            List<URL> urls = new ArrayList<>();
            urls.add(file.toURI().toURL());
            // Forge bundles dependencies in META-INF/jarjar/; NeoForge and
            // Fabric-style bundles use META-INF/jars/. Both are real nested
            // archives that must be on the mod's class path.
            urls.addAll(nestedJarUrls(file, loaded));
            URLClassLoader loader = new URLClassLoader(
                    urls.toArray(new URL[0]),
                    ModManager.class.getClassLoader());
            loaded.loader = loader;
            List<Class<?>> classes = new ArrayList<>();
            try (JarFile jar = new JarFile(file)) {
                Enumeration<JarEntry> entries = jar.entries();
                while (entries.hasMoreElements()) {
                    JarEntry entry = entries.nextElement();
                    String name = entry.getName();
                    if (!name.endsWith(".class")
                            || name.startsWith("META-INF/versions/")
                            || name.endsWith("module-info.class")
                            || name.endsWith("package-info.class")) {
                        continue;
                    }
                    String binary = name.substring(0, name.length() - ".class".length()).replace('/', '.');
                    try {
                        classes.add(Class.forName(binary, false, loader));
                    } catch (Throwable throwable) {
                        loaded.classErrors.add(binary + ": " + throwable);
                    }
                }
            }
            loaded.classCount = classes.size();
            for (Class<?> clazz : classes) {
                Mod annotation = clazz.getAnnotation(Mod.class);
                if (annotation == null) {
                    continue;
                }
                String id = annotation.value();
                if (id == null || id.isBlank()) {
                    id = annotation.modid();
                }
                if (id == null || id.isBlank()) {
                    id = deriveId(file.getName());
                    loaded.warnings.add("empty @Mod value; using jar-derived id " + id);
                }
                if (byId.containsKey(id)) {
                    loaded.warnings.add("duplicate mod id " + id + "; " + clazz.getName() + " was rejected");
                    continue;
                }
                String version = annotation.version() == null ? "" : annotation.version();
                if (version.isBlank()) {
                    version = jarVersion(file);
                }
                VibeMod mod = new VibeMod(id, version, path, clazz);
                mods.add(mod);
                byId.put(id, mod);
                loaded.modIds.add(id);
                loaded.entrypoints.add(clazz.getName());
            }
        } catch (Throwable throwable) {
            loaded.error = "could not inspect " + path + ": " + throwable;
            failures.add(loaded.error);
        }
        return loaded;
    }

    /**
     * Extract nested jars from a mod archive into per-archive temp directories.
     * The temp directories are removed by {@link #reset()}.
     */
    private List<URL> nestedJarUrls(File file, LoadedJar loaded) {
        List<URL> urls = new ArrayList<>();
        try (JarFile jar = new JarFile(file)) {
            Enumeration<JarEntry> entries = jar.entries();
            while (entries.hasMoreElements()) {
                JarEntry entry = entries.nextElement();
                String name = entry.getName();
                if (entry.isDirectory() || !name.endsWith(".jar")) {
                    continue;
                }
                if (!name.startsWith("META-INF/jarjar/") && !name.startsWith("META-INF/jars/")) {
                    continue;
                }
                try {
                    Path dir = Files.createTempDirectory("vibe-nested-");
                    tempDirs.add(dir);
                    String fileName = name.substring(name.lastIndexOf('/') + 1);
                    Path target = dir.resolve(fileName);
                    try (InputStream input = jar.getInputStream(entry)) {
                        Files.copy(input, target, StandardCopyOption.REPLACE_EXISTING);
                    }
                    urls.add(target.toUri().toURL());
                    loaded.nestedJars.add(name);
                } catch (Exception nested) {
                    loaded.warnings.add("could not extract nested jar " + name + ": " + nested);
                }
            }
        } catch (Exception ignored) {
            // The main archive scan reports unreadable jars.
        }
        return urls;
    }

    private void construct(VibeMod mod) {
        ModLoadingContext.setActive(null);
        VibeModContainer container = new VibeModContainer(mod.id, mod.version, null);
        FMLJavaModLoadingContext context = new FMLJavaModLoadingContext(container, mod.eventBus);
        container.attach(null);
        ModLoadingContext.setActive(context);
        try {
            Constructor<?> constructor;
            Object[] args;
            try {
                constructor = mod.modClass.getDeclaredConstructor(FMLJavaModLoadingContext.class);
                args = new Object[] {context};
            } catch (NoSuchMethodException notFml) {
                try {
                    constructor = mod.modClass.getDeclaredConstructor(IEventBus.class);
                    args = new Object[] {mod.eventBus};
                } catch (NoSuchMethodException notBus) {
                    constructor = mod.modClass.getDeclaredConstructor();
                    args = new Object[0];
                }
            }
            constructor.setAccessible(true);
            Object instance = constructor.newInstance(args);
            mod.instance = instance;
            mod.container = container;
            container.attach(instance);
            mod.status = "constructed";
            mod.eventBus.register(instance);
        } catch (Throwable throwable) {
            Throwable cause = unwrap(throwable);
            mod.status = "construct_failed";
            mod.error = cause.toString();
            failures.add(mod.id + ": " + cause);
        } finally {
            ModLoadingContext.clearActive();
        }
    }

    private void post(VibeMod mod, Event event) {
        ModLoadingContext.setActive(new FMLJavaModLoadingContext(mod.container, mod.eventBus));
        try {
            mod.eventBus.post(event);
            mod.lifecycleInvocations++;
        } catch (Throwable throwable) {
            mod.error = unwrap(throwable).toString();
            failures.add(mod.id + " during " + event.getClass().getSimpleName() + ": " + mod.error);
        } finally {
            ModLoadingContext.clearActive();
        }
    }

    private void invokeLegacy(VibeMod mod, Event event) {
        for (Method method : mod.modClass.getDeclaredMethods()) {
            if (!method.isAnnotationPresent(Mod.EventHandler.class)) {
                continue;
            }
            if (method.getParameterCount() != 1) {
                continue;
            }
            Class<?> parameter = method.getParameterTypes()[0];
            if (!parameter.isAssignableFrom(event.getClass())) {
                continue;
            }
            String label = mod.id + "#" + method.getName();
            ModLoadingContext.setActive(new FMLJavaModLoadingContext(mod.container, mod.eventBus));
            try {
                method.setAccessible(true);
                method.invoke(mod.instance, event);
                mod.eventBus.recordExternal(event.getClass().getSimpleName(), label, true, null);
                mod.lifecycleInvocations++;
            } catch (Throwable throwable) {
                Throwable cause = unwrap(throwable);
                mod.eventBus.recordExternal(event.getClass().getSimpleName(), label, false, cause.toString());
                failures.add(label + " failed: " + cause);
            } finally {
                ModLoadingContext.clearActive();
            }
        }
    }

    private void registerEventBusSubscribers() {
        for (LoadedJar jar : jars) {
            if (jar.loader == null) {
                continue;
            }
            List<Class<?>> classes;
            try {
                classes = classesFrom(jar);
            } catch (RuntimeException ignored) {
                continue;
            }
            for (Class<?> clazz : classes) {
                Mod.EventBusSubscriber annotation = clazz.getAnnotation(Mod.EventBusSubscriber.class);
                if (annotation == null || clazz.isAnnotationPresent(Mod.class)) {
                    continue;
                }
                if (!sideMatches(annotation.value())) {
                    continue;
                }
                String modId = annotation.modid();
                VibeEventBus target;
                if (annotation.bus() == Mod.Bus.MOD) {
                    VibeMod mod = !modId.isBlank() ? byId.get(modId) : firstModFor(jar);
                    if (mod == null) {
                        continue;
                    }
                    target = mod.eventBus;
                } else {
                    target = forgeBus;
                }
                target.register(clazz);
            }
        }
    }

    private boolean sideMatches(Mod.Dist[] dists) {
        if (dists == null || dists.length == 0) {
            return true;
        }
        Mod.Dist current = "CLIENT".equals(side) ? Mod.Dist.CLIENT : Mod.Dist.DEDICATED_SERVER;
        for (Mod.Dist dist : dists) {
            if (dist == current) {
                return true;
            }
        }
        return false;
    }

    private List<Class<?>> classesFrom(LoadedJar jar) {
        // Classes discovered during load are not retained on LoadedJar to keep
        // memory bounded; re-scan the jar and load without initialisation.
        List<Class<?>> classes = new ArrayList<>();
        try (JarFile file = new JarFile(jar.path.toFile())) {
            Enumeration<JarEntry> entries = file.entries();
            while (entries.hasMoreElements()) {
                String name = entries.nextElement().getName();
                if (!name.endsWith(".class")
                        || name.startsWith("META-INF/versions/")
                        || name.endsWith("module-info.class")
                        || name.endsWith("package-info.class")) {
                    continue;
                }
                String binary = name.substring(0, name.length() - ".class".length()).replace('/', '.');
                try {
                    classes.add(Class.forName(binary, false, jar.loader));
                } catch (Throwable ignored) {
                    // Already reported during the main scan.
                }
            }
        } catch (Exception ignored) {
            // Already reported during the main scan.
        }
        return classes;
    }

    private VibeMod firstModFor(LoadedJar jar) {
        for (VibeMod mod : mods) {
            if (mod.path.equals(jar.path)) {
                return mod;
            }
        }
        return null;
    }

    private List<Object> statusList() {
        List<Object> list = new ArrayList<>();
        for (VibeMod mod : mods) {
            list.add(mod.describe());
        }
        return list;
    }

    private List<Map<String, Object>> allRecords() {
        List<Map<String, Object>> records = new ArrayList<>(forgeBus.records());
        for (VibeMod mod : mods) {
            records.addAll(mod.eventBus.records());
        }
        return records;
    }

    private List<ModContainer> containers() {
        List<ModContainer> containers = new ArrayList<>();
        for (VibeMod mod : mods) {
            if (mod.container != null) {
                containers.add(mod.container);
            }
        }
        return containers;
    }

    private void reset() {
        for (Path dir : tempDirs) {
            deleteRecursively(dir);
        }
        tempDirs.clear();
        mods.clear();
        byId.clear();
        jars.clear();
        failures.clear();
        phase = "new";
        profile = null;
        backend = null;
        VibeRegistryManager.reset();
    }

    private static void deleteRecursively(Path dir) {
        try (var stream = Files.walk(dir)) {
            stream.sorted(Comparator.reverseOrder()).forEach(path -> {
                try {
                    Files.deleteIfExists(path);
                } catch (Exception ignored) {
                    // Best effort cleanup; a leftover temp file is not fatal.
                }
            });
        } catch (Exception ignored) {
            // Best effort cleanup.
        }
    }

    private static Throwable unwrap(Throwable throwable) {
        if (throwable instanceof InvocationTargetException invocation && invocation.getCause() != null) {
            return invocation.getCause();
        }
        return throwable;
    }

    private static String jarVersion(File file) {
        try (JarFile jar = new JarFile(file)) {
            String version = jar.getManifest() == null
                    ? null
                    : jar.getManifest().getMainAttributes().getValue("Implementation-Version");
            return version == null ? "0.0.0" : version;
        } catch (Exception ignored) {
            return "0.0.0";
        }
    }

    private static String deriveId(String fileName) {
        String stem = fileName.toLowerCase().replaceFirst("\\.jar$", "");
        stem = stem.replaceAll("[^a-z0-9_]+", "_");
        if (stem.isEmpty()) {
            stem = "vibe_mod";
        }
        if (!Character.isLetter(stem.charAt(0))) {
            stem = "m" + stem;
        }
        return stem.length() > 64 ? stem.substring(0, 64) : stem;
    }

    private static final class VibeModContainer extends ModContainer {
        private VibeModContainer(String modId, String version, Object mod) {
            super(modId, version, mod);
        }

        private void attach(Object mod) {
            setMod(mod);
        }
    }

    private static final class VibeMod {
        private final String id;
        private final String version;
        private final Path path;
        private final Class<?> modClass;
        private final VibeEventBus eventBus;
        private final List<String> classErrors = new ArrayList<>();
        private Object instance;
        private VibeModContainer container;
        private String status = "loaded";
        private String error;
        private int lifecycleInvocations;

        private VibeMod(String id, String version, Path path, Class<?> modClass) {
            this.id = id;
            this.version = version;
            this.path = path;
            this.modClass = modClass;
            this.eventBus = new VibeEventBus("mod:" + id);
        }

        private boolean isUsable() {
            return "loaded".equals(status) || "constructed".equals(status);
        }

        private Map<String, Object> describe() {
            Map<String, Object> map = new LinkedHashMap<>();
            map.put("id", id);
            map.put("version", version);
            map.put("path", path.toString());
            map.put("entrypoint", modClass.getName());
            map.put("status", status);
            map.put("constructed", instance != null);
            map.put("lifecycle_invocations", lifecycleInvocations);
            map.put("handler_count", eventBus.handlerCount());
            if (error != null) {
                map.put("error", error);
            }
            if (!classErrors.isEmpty()) {
                map.put("class_errors", new ArrayList<>(classErrors));
            }
            return map;
        }
    }

    private static final class LoadedJar {
        private final Path path;
        private URLClassLoader loader;
        private int classCount;
        private final List<String> entrypoints = new ArrayList<>();
        private final List<String> modIds = new ArrayList<>();
        private final List<String> classErrors = new ArrayList<>(0);
        private final List<String> warnings = new ArrayList<>();
        private final List<String> nestedJars = new ArrayList<>();
        private String error;

        private LoadedJar(Path path) {
            this.path = path;
        }

        private Map<String, Object> describe() {
            Map<String, Object> map = new LinkedHashMap<>();
            map.put("path", path.toString());
            map.put("class_count", classCount);
            map.put("entrypoints", new ArrayList<>(entrypoints));
            map.put("mod_ids", new ArrayList<>(modIds));
            map.put("nested_jars_loaded", new ArrayList<>(nestedJars));
            map.put("warnings", new ArrayList<>(warnings));
            map.put("class_errors", classErrors.size() > 50
                    ? new ArrayList<>(classErrors.subList(0, 50))
                    : new ArrayList<>(classErrors));
            if (error != null) {
                map.put("error", error);
            }
            return map;
        }
    }
}
