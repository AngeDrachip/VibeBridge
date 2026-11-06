package dev.vibe.runtime;

import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

import dev.vibe.runtime.json.Json;

/**
 * Vibe's Java-side bridge process.
 *
 * <p>The runtime speaks newline-delimited JSON on stdin/stdout. Every line is
 * exactly one protocol message; mod logging and accidental {@code System.out}
 * writes are redirected to stderr so they can never corrupt the protocol.
 *
 * <p>This entrypoint only dispatches requests. Forge compatibility lives in
 * {@link ModManager} and Vibe's clean-room Forge API subset.
 */
public final class VibeRuntime {
    /** Must match {@code vibe_bridge::protocol::PROTOCOL_VERSION}. */
    public static final int PROTOCOL_VERSION = 1;
    public static final String RUNTIME_NAME = "vibe-runtime";
    public static final String RUNTIME_VERSION = "0.1.0";

    /**
     * The Forge API names Vibe implements. Kept in sync with
     * {@code vibe_core::analysis::SUPPORTED_FORGE_API}; it is reported to the
     * Rust side during the handshake so the two lists can be compared.
     */
    public static final List<String> SUPPORTED_FORGE_API = List.of(
            "net.minecraftforge.common.MinecraftForge",
            "net.minecraftforge.eventbus.api.Event",
            "net.minecraftforge.eventbus.api.EventPriority",
            "net.minecraftforge.eventbus.api.IEventBus",
            "net.minecraftforge.eventbus.api.SubscribeEvent",
            "net.minecraftforge.eventbus.api.bus.BusGroup",
            "net.minecraftforge.eventbus.api.bus.EventBus",
            "net.minecraftforge.eventbus.api.bus.IModBusEventMarker",
            "net.minecraftforge.eventbus.api.listener.SubscribeEvent",
            "net.minecraftforge.fml.ModContainer",
            "net.minecraftforge.fml.ModList",
            "net.minecraftforge.fml.ModLoadingContext",
            "net.minecraftforge.fml.common.FMLCommonHandler",
            "net.minecraftforge.fml.common.Mod",
            "net.minecraftforge.fml.common.Mod$Bus",
            "net.minecraftforge.fml.common.Mod$Dist",
            "net.minecraftforge.fml.common.Mod$EventBusSubscriber",
            "net.minecraftforge.fml.common.Mod$EventHandler",
            "net.minecraftforge.fml.common.event.FMLConstructionEvent",
            "net.minecraftforge.fml.common.event.FMLInitializationEvent",
            "net.minecraftforge.fml.common.event.FMLLoadCompleteEvent",
            "net.minecraftforge.fml.common.event.FMLPostInitializationEvent",
            "net.minecraftforge.fml.common.event.FMLPreInitializationEvent",
            "net.minecraftforge.fml.common.event.FMLServerStartingEvent",
            "net.minecraftforge.fml.event.IModBusEvent",
            "net.minecraftforge.fml.event.lifecycle.FMLClientSetupEvent",
            "net.minecraftforge.fml.event.lifecycle.FMLCommonSetupEvent",
            "net.minecraftforge.fml.event.lifecycle.FMLConstructModEvent",
            "net.minecraftforge.fml.event.lifecycle.FMLDedicatedServerSetupEvent",
            "net.minecraftforge.fml.event.lifecycle.FMLLoadCompleteEvent",
            "net.minecraftforge.fml.event.lifecycle.InterModEnqueueEvent",
            "net.minecraftforge.fml.event.lifecycle.InterModProcessEvent",
            "net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext");

    private final ModManager mods = new ModManager();
    private final PrintStream protocolOut;
    private final BufferedReader protocolIn;
    private String side;

    private VibeRuntime(PrintStream protocolOut, BufferedReader protocolIn) {
        this.protocolOut = protocolOut;
        this.protocolIn = protocolIn;
        String configured = System.getProperty("vibe.side", "SERVER");
        this.side = configured.isBlank() ? "SERVER" : configured.toUpperCase();
    }

    public static void main(String[] args) throws Exception {
        // Reserve the real stdout for the protocol, then point System.out at
        // stderr. Any mod that prints (or any shim that logs to stdout) becomes
        // diagnostics instead of a protocol violation.
        PrintStream protocol = new PrintStream(
                new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        PrintStream diagnostics = new PrintStream(
                new FileOutputStream(FileDescriptor.err), true, StandardCharsets.UTF_8);
        System.setOut(diagnostics);
        System.setErr(diagnostics);

        if (Arrays.asList(args).contains("--help") || Arrays.asList(args).contains("-h")) {
            diagnostics.println("Vibe Java runtime " + RUNTIME_VERSION);
            diagnostics.println("Usage: java -cp <vibe-runtime> dev.vibe.runtime.VibeRuntime --stdio");
            diagnostics.println("Communicates with the Rust host over JSON lines on stdin/stdout.");
            return;
        }

        BufferedReader input = new BufferedReader(
                new InputStreamReader(System.in, StandardCharsets.UTF_8));
        VibeRuntime runtime = new VibeRuntime(protocol, input);
        runtime.run();
    }

    private void run() {
        send(Json.object(
                "type", "hello",
                "protocol", PROTOCOL_VERSION,
                "runtime", RUNTIME_NAME + "/" + RUNTIME_VERSION,
                "java_version", System.getProperty("java.version", "unknown"),
                "java_vendor", System.getProperty("java.vendor", "unknown"),
                "os", System.getProperty("os.name", "unknown"),
                "arch", System.getProperty("os.arch", "unknown"),
                "pid", ProcessHandle.current().pid(),
                "supported_forge_api", new ArrayList<>(SUPPORTED_FORGE_API)));

        boolean welcomed = false;
        boolean running = true;
        while (running) {
            String line;
            try {
                line = protocolIn.readLine();
            } catch (Exception error) {
                System.err.println("[vibe] could not read bridge input: " + error);
                break;
            }
            if (line == null) {
                break;
            }
            line = line.trim();
            if (line.isEmpty()) {
                continue;
            }

            Map<String, Object> message;
            try {
                message = Json.asObject(Json.parse(line));
            } catch (RuntimeException error) {
                System.err.println("[vibe] ignoring malformed bridge line: " + error);
                continue;
            }
            String type = Json.string(message, "type");
            if (type == null) {
                System.err.println("[vibe] ignoring bridge message without a type");
                continue;
            }
            switch (type) {
                case "welcome" -> {
                    long protocol = Json.number(message, "protocol", -1);
                    if (protocol != PROTOCOL_VERSION) {
                        send(Json.object(
                                "type", "goodbye",
                                "reason", "protocol mismatch: host speaks " + protocol
                                        + ", runtime speaks " + PROTOCOL_VERSION));
                        return;
                    }
                    welcomed = true;
                    send(Json.object(
                            "type", "event",
                            "event", "runtime.ready",
                            "data", Json.object(
                                    "runtime", RUNTIME_NAME + "/" + RUNTIME_VERSION,
                                    "protocol", PROTOCOL_VERSION,
                                    "side", side)));
                }
                case "request" -> {
                    if (!welcomed) {
                        System.err.println("[vibe] request before welcome; accepting anyway");
                        welcomed = true;
                    }
                    if (!handleRequest(message)) {
                        running = false;
                    }
                }
                case "goodbye" -> {
                    running = false;
                }
                default -> System.err.println("[vibe] ignoring unsupported message type: " + type);
            }
        }
        send(Json.object("type", "goodbye", "reason", "runtime stopped"));
    }

    private boolean handleRequest(Map<String, Object> message) {
        long id = Json.number(message, "id", -1);
        String method = Json.string(message, "method");
        Object rawParams = message.get("params");
        Map<String, Object> params;
        if (rawParams == null) {
            params = Json.object();
        } else {
            try {
                params = Json.asObject(rawParams);
            } catch (RuntimeException error) {
                respondError(id, "invalid_params", "params must be a JSON object", error.toString());
                return true;
            }
        }
        if (method == null) {
            respondError(id, "invalid_request", "request is missing a method", null);
            return true;
        }

        try {
            switch (method) {
                case "runtime.ping" -> {
                    respond(id, Json.object(
                            "ok", true,
                            "runtime", RUNTIME_NAME + "/" + RUNTIME_VERSION,
                            "protocol", PROTOCOL_VERSION));
                    return true;
                }
                case "runtime.info" -> {
                    respond(id, runtimeInfo());
                    return true;
                }
                case "mods.load" -> {
                    List<Path> paths = readPaths(params);
                    String requestedSide = Json.string(params, "side");
                    if (requestedSide != null && !requestedSide.isBlank()) {
                        side = requestedSide.toUpperCase();
                    }
                    respond(id, mods.load(paths, side));
                    return true;
                }
                case "lifecycle.start" -> {
                    respond(id, mods.startLifecycle());
                    return true;
                }
                case "mods.status" -> {
                    respond(id, mods.status());
                    return true;
                }
                case "registry.snapshot" -> {
                    respond(id, mods.registrySnapshot());
                    return true;
                }
                case "runtime.shutdown" -> {
                    respond(id, Json.object("ok", true, "stopped", true));
                    return false;
                }
                default -> {
                    respondError(id, "unknown_method", "unknown bridge method: " + method, null);
                    return true;
                }
            }
        } catch (IllegalArgumentException error) {
            respondError(id, "invalid_params", error.getMessage(), null);
            return true;
        } catch (Throwable error) {
            respondError(id, "internal_error", error.toString(), stackTrace(error));
            return true;
        }
    }

    private Map<String, Object> runtimeInfo() {
        return Json.object(
                "runtime", RUNTIME_NAME + "/" + RUNTIME_VERSION,
                "protocol", PROTOCOL_VERSION,
                "side", side,
                "java", Json.object(
                        "version", System.getProperty("java.version", "unknown"),
                        "vendor", System.getProperty("java.vendor", "unknown"),
                        "home", System.getProperty("java.home", "unknown")),
                "os", Json.object(
                        "name", System.getProperty("os.name", "unknown"),
                        "version", System.getProperty("os.version", "unknown"),
                        "arch", System.getProperty("os.arch", "unknown")),
                "pid", ProcessHandle.current().pid(),
                "supported_forge_api", new ArrayList<>(SUPPORTED_FORGE_API),
                "limitations", List.of(
                        "No Minecraft classes are provided.",
                        "Mixin, coremods, access transformers, and nested jars are not executed.",
                        "Only the Forge API names in supported_forge_api are present."));
    }

    private static List<Path> readPaths(Map<String, Object> params) {
        Object raw = params.get("paths");
        if (!(raw instanceof List<?> list)) {
            throw new IllegalArgumentException("mods.load requires a 'paths' array");
        }
        List<Path> paths = new ArrayList<>();
        for (Object entry : list) {
            if (entry == null) {
                throw new IllegalArgumentException("'paths' contains a null entry");
            }
            paths.add(Path.of(String.valueOf(entry)));
        }
        return paths;
    }

    private void respond(long id, Object result) {
        send(Json.object(
                "type", "response",
                "id", id,
                "ok", true,
                "result", result));
    }

    private void respondError(long id, String code, String message, String details) {
        Map<String, Object> error = Json.object(
                "code", code == null ? "error" : code,
                "message", message == null ? "unknown error" : message);
        if (details != null) {
            error.put("details", details);
        }
        send(Json.object(
                "type", "response",
                "id", id,
                "ok", false,
                "result", null,
                "error", error));
    }

    private synchronized void send(Map<String, Object> message) {
        protocolOut.println(Json.write(message));
        protocolOut.flush();
    }

    private static String stackTrace(Throwable error) {
        StringBuilder out = new StringBuilder();
        for (StackTraceElement frame : error.getStackTrace()) {
            if (out.length() > 4000) {
                out.append("\n...");
                break;
            }
            out.append("\n  at ").append(frame);
        }
        Throwable cause = error.getCause();
        if (cause != null && cause != error) {
            out.append("\ncaused by: ").append(cause);
        }
        return out.toString();
    }
}
