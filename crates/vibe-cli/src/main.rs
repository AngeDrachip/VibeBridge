//! The Vibe command line.
//!
//! Every command is deliberately explicit about what Vibe did and did not do.
//! `inspect` and `plan` never execute mod code; `run` only starts jars whose
//! analysis says Vibe's Forge API subset is sufficient; `horizon export`
//! carries resources, never Java code.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use vibe_bridge::{
    default_runtime_path, detect_all_java, locate_java, RuntimeClient, RuntimeOptions,
};
use vibe_core::error::{Result, VibeError};
use vibe_core::metadata::ModSide;
use vibe_core::Verdict;
use vibe_horizon::{
    export_jar, install_content_bundle, probe, validate_content_bundle, ExportOptions,
};
use vibe_loader::{discover_jars, inspect_jar, JarInspection};
use vibe_registry::plan_for_side;

#[derive(Parser)]
#[command(
    name = "vibe",
    version,
    about = "Run Forge mods on Horizon. Without porting.",
    long_about = "Vibe inspects Forge mod jars without running them, plans a load, executes the \
subset of the Forge API it implements in a real JVM, and exports assets/data as a Horizon \
content mod. It never modifies the original jars and never claims untested compatibility."
)]
struct Cli {
    /// Print machine-readable JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check Java, the Vibe runtime, and Horizon availability.
    Doctor(DoctorArgs),
    /// Inspect mod jars without executing any code.
    Inspect(InspectArgs),
    /// Build a dependency-ordered load plan.
    Plan(PlanArgs),
    /// Load mods in the Java runtime and run the Forge lifecycle subset.
    Run(RunArgs),
    /// Work with a Horizon installation.
    Horizon(HorizonArgs),
}

#[derive(Args)]
struct DoctorArgs {
    /// Java executable or JDK/JRE home to use.
    #[arg(long)]
    java: Option<PathBuf>,
    /// Vibe runtime jar or classes directory.
    #[arg(long)]
    runtime: Option<PathBuf>,
    /// horizon-ctl executable or installation folder.
    #[arg(long)]
    ctl: Option<PathBuf>,
}

#[derive(Args)]
struct InspectArgs {
    /// Jars or directories containing jars.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

#[derive(Args)]
struct PlanArgs {
    /// Jars or directories containing jars.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    /// Plan for one side. Mods and dependencies declared for the other side
    /// are reported but not required.
    #[arg(long, value_parser = ["CLIENT", "SERVER", "client", "server"])]
    side: Option<String>,
}

#[derive(Args)]
struct RunArgs {
    /// Jars or directories containing jars.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    #[arg(long)]
    java: Option<PathBuf>,
    /// Vibe runtime jar or classes directory.
    #[arg(long)]
    runtime: Option<PathBuf>,
    /// Forge side passed to the runtime.
    #[arg(long, default_value = "SERVER", value_parser = ["SERVER", "CLIENT", "server", "client"])]
    side: String,
    #[arg(long, default_value_t = 2048)]
    heap_mb: u32,
    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,
    /// Also hand jars that analysis blocked to the runtime. Diagnostics only:
    /// blocked jars normally reference classes Vibe does not provide.
    #[arg(long)]
    include_blocked: bool,
}

#[derive(Args)]
struct HorizonArgs {
    #[command(subcommand)]
    command: HorizonCommand,
}

#[derive(Subcommand)]
enum HorizonCommand {
    /// Locate horizon-ctl and ask it whether a game is reachable.
    Probe(ProbeArgs),
    /// Export a jar's assets/data as a Horizon .hmod content bundle.
    Export(ExportArgs),
    /// Check a .hmod against the Horizon rules Vibe can verify.
    Validate(ValidateArgs),
    /// Copy a validated .hmod into <game-dir>/horizon-mods/.
    Install(InstallArgs),
}

#[derive(Args)]
struct ProbeArgs {
    #[arg(long)]
    ctl: Option<PathBuf>,
}

#[derive(Args)]
struct ExportArgs {
    /// The Forge mod jar to read. It is never modified.
    jar: PathBuf,
    /// Destination .hmod file.
    #[arg(short, long)]
    output: PathBuf,
    /// Bundle id. Required when the jar declares more than one mod.
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    version: Option<String>,
    /// client, server, or both.
    #[arg(long, value_parser = ["client", "server", "both"])]
    side: Option<String>,
    /// Extra namespace to claim (repeatable), for jars whose resources live
    /// beside their mod id (for example `kaisyn`).
    #[arg(long = "namespace")]
    namespaces: Vec<String>,
    #[arg(long)]
    force: bool,
    /// Leave out additions to vanilla tags.
    #[arg(long)]
    no_vanilla_tags: bool,
    /// Leave out convention-tag additions.
    #[arg(long)]
    no_convention_tags: bool,
    /// Leave out atlas sources.
    #[arg(long)]
    no_atlases: bool,
}

#[derive(Args)]
struct ValidateArgs {
    /// The .hmod content bundle to check. It is opened read-only.
    bundle: PathBuf,
}

#[derive(Args)]
struct InstallArgs {
    /// The .hmod content bundle to install.
    bundle: PathBuf,
    /// Horizon game folder: the one holding options.txt.
    #[arg(long)]
    game_dir: PathBuf,
    /// Replace an existing file with the same name.
    #[arg(long)]
    force: bool,
    /// Check everything and report the destination without copying.
    #[arg(long)]
    dry_run: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match execute(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("vibe: {error}");
            ExitCode::from(1)
        }
    }
}

fn execute(cli: Cli) -> Result<u8> {
    let json = cli.json;
    match cli.command {
        Command::Doctor(args) => doctor(args, json),
        Command::Inspect(args) => inspect(args, json),
        Command::Plan(args) => make_plan(args, json),
        Command::Run(args) => run(args, json),
        Command::Horizon(args) => match args.command {
            HorizonCommand::Probe(args) => horizon_probe(args, json),
            HorizonCommand::Export(args) => horizon_export(args, json),
            HorizonCommand::Validate(args) => horizon_validate(args, json),
            HorizonCommand::Install(args) => horizon_install(args, json),
        },
    }
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

fn doctor(args: DoctorArgs, json: bool) -> Result<u8> {
    let selected = locate_java(args.java.as_deref());
    let installations = detect_all_java();
    let runtime = resolve_runtime(args.runtime);
    let horizon = probe(args.ctl.as_deref());

    let java_value = match &selected {
        Ok(java) => json!({
            "executable": java.executable,
            "version": java.version,
            "major": java.major,
            "vendor": java.vendor,
        }),
        Err(error) => json!({ "error": error.to_string() }),
    };
    let runtime_value = match &runtime {
        Some(path) => json!({
            "path": path,
            "exists": path.exists(),
            "kind": if path.is_dir() { "classes" } else { "jar" },
        }),
        None => Value::Null,
    };
    let report = json!({
        "java": java_value,
        "java_installations": installations,
        "runtime": runtime_value,
        "horizon": horizon,
        "healthy": selected.is_ok() && runtime.as_ref().map(|path| path.exists()).unwrap_or(false),
    });

    if json {
        print_json(&report)?;
    } else {
        println!("Vibe doctor");
        match &selected {
            Ok(java) => println!(
                "  Java:    {} (Java {}, {})",
                java.executable.display(),
                java.version,
                java.vendor
            ),
            Err(error) => println!("  Java:    NOT USABLE — {error}"),
        }
        if !installations.is_empty() {
            println!("  Found:");
            for java in &installations {
                println!(
                    "           Java {:<3} {} ({})",
                    java.major,
                    java.executable.display(),
                    java.vendor
                );
            }
        }
        match &runtime {
            Some(path) if path.exists() => {
                println!(
                    "  Runtime: {} ({})",
                    path.display(),
                    if path.is_dir() { "classes" } else { "jar" }
                );
            }
            Some(path) => println!("  Runtime: MISSING at {}", path.display()),
            None => println!("  Runtime: MISSING (build java/ and pass --runtime)"),
        }
        match &horizon.ctl {
            Some(ctl) => {
                println!(
                    "  Horizon: {} ({})",
                    ctl.display(),
                    if horizon.game_reachable {
                        "a game answered"
                    } else {
                        "installed, no game answering"
                    }
                );
                if let Some(summary) = &horizon.help_summary {
                    println!("           {summary}");
                }
            }
            None => println!("  Horizon: horizon-ctl not found"),
        }
        for note in &horizon.notes {
            println!("           {note}");
        }
        if selected.is_err() {
            println!("\nVibe needs Java 17 or newer. Install a JDK, or set VIBE_JAVA/JAVA_HOME.");
        }
        if runtime.as_ref().map(|path| !path.exists()).unwrap_or(true) {
            println!("Build the Java side with `java/build.ps1` (Windows) or `java/build.sh`, then rerun doctor.");
        }
    }

    let healthy = selected.is_ok() && runtime.as_ref().map(|path| path.exists()).unwrap_or(false);
    Ok(if healthy { 0 } else { 1 })
}

// ---------------------------------------------------------------------------
// inspect / plan
// ---------------------------------------------------------------------------

fn inspect(args: InspectArgs, json: bool) -> Result<u8> {
    let discovery = discover_jars(&args.paths)?;
    let mut inspections = Vec::new();
    let mut errors = Vec::new();
    for jar in &discovery.jars {
        match inspect_jar(jar, None) {
            Ok(inspection) => inspections.push(inspection),
            Err(error) => errors.push(json!({ "path": jar, "error": error.to_string() })),
        }
    }
    for missing in &discovery.missing_paths {
        errors.push(json!({ "path": missing, "error": "path does not exist" }));
    }

    if json {
        print_json(&json!({
            "jars": inspections,
            "errors": errors,
            "skipped": discovery.skipped,
            "missing_paths": discovery.missing_paths,
        }))?;
    } else {
        if inspections.is_empty() && errors.is_empty() {
            println!("No jars found.");
        }
        for inspection in &inspections {
            print_inspection(inspection);
        }
        for error in &errors {
            println!(
                "ERROR  {}",
                error.get("path").and_then(Value::as_str).unwrap_or("?")
            );
            println!(
                "       {}",
                error.get("error").and_then(Value::as_str).unwrap_or("?")
            );
        }
        for skipped in &discovery.skipped {
            println!("SKIP   {} (not a .jar)", skipped.display());
        }
    }
    Ok(if errors.is_empty() { 0 } else { 1 })
}

fn print_inspection(inspection: &JarInspection) {
    let ids = inspection.mod_ids();
    let ids = if ids.is_empty() {
        "<no declared id>".to_string()
    } else {
        ids.join(", ")
    };
    println!(
        "{}  [{}]  {}",
        inspection.file_name,
        verdict_label(inspection.report.verdict),
        ids
    );
    println!(
        "  {} bytes, {} classes, loader: {:?}, side: {:?}",
        inspection.size_bytes,
        inspection.class_count,
        inspection.metadata.loader,
        inspection
            .metadata
            .mods
            .first()
            .map(|m| m.side)
            .unwrap_or_default()
    );
    if !inspection.entrypoints.is_empty() {
        println!("  entrypoints: {}", inspection.entrypoints.join(", "));
    }
    if inspection.has_assets || inspection.has_data {
        println!(
            "  resources: assets={} data={} (namespaces: {})",
            inspection.has_assets,
            inspection.has_data,
            inspection
                .asset_namespaces
                .iter()
                .chain(inspection.data_namespaces.iter())
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for blocker in &inspection.report.blockers {
        println!("  {:?}: {}", blocker.severity, blocker.message);
        if blocker.evidence_total > 1 && !blocker.evidence_groups.is_empty() {
            for group in blocker.evidence_groups.iter().take(8) {
                println!(
                    "    {} ({}): {}",
                    group.label,
                    group.count,
                    group.examples.join(", ")
                );
            }
            let mut notes = Vec::new();
            if blocker.evidence_groups.len() > 8 {
                notes.push(format!(
                    "{} more group(s)",
                    blocker.evidence_groups.len() - 8
                ));
            }
            if blocker.evidence_groups_truncated {
                notes.push(format!(
                    "only the largest {} groups are listed",
                    blocker.evidence_groups.len()
                ));
            }
            if blocker.evidence_total > blocker.evidence.len() {
                notes.push(format!(
                    "showing {} of {} evidence entries",
                    blocker.evidence.len(),
                    blocker.evidence_total
                ));
            }
            if !notes.is_empty() {
                println!("    {}; full details in --json", notes.join("; "));
            }
        } else {
            for evidence in blocker.evidence.iter().take(6) {
                println!("    - {evidence}");
            }
        }
    }
    println!();
}

fn make_plan(args: PlanArgs, json: bool) -> Result<u8> {
    let discovery = discover_jars(&args.paths)?;
    let mut inspections = Vec::new();
    let mut errors = Vec::new();
    for jar in &discovery.jars {
        match inspect_jar(jar, None) {
            Ok(inspection) => inspections.push(inspection),
            Err(error) => errors.push(json!({ "path": jar, "error": error.to_string() })),
        }
    }
    let side = args.side.as_deref().map(ModSide::parse);
    let plan = plan_for_side(&inspections, side);

    if json {
        print_json(&json!({
            "plan": plan,
            "errors": errors,
            "missing_paths": discovery.missing_paths,
        }))?;
    } else {
        match plan.side {
            Some(side) => println!("Vibe load plan for {}", side.as_str()),
            None => println!("Vibe load plan"),
        }
        println!("  mods: {}", plan.mods.len());
        for excluded in &plan.excluded_mods {
            println!("  EXCLUDED {}: {}", excluded.id, excluded.reason);
        }
        println!(
            "  load order: {}",
            plan.load_order
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" -> ")
        );
        for module in &plan.mods {
            println!(
                "  {:<24} {:<12} {}",
                module.id.to_string(),
                verdict_label(module.verdict),
                module.jar.display()
            );
        }
        for duplicate in &plan.duplicate_ids {
            println!("  DUPLICATE id {}: {:?}", duplicate.id, duplicate.jars);
        }
        for missing in &plan.missing_dependencies {
            println!(
                "  MISSING {} needs {}{} [side {}, range {}]",
                missing.owner,
                missing.dependency,
                if missing.mandatory {
                    " (required)"
                } else {
                    " (optional)"
                },
                missing.side.as_str(),
                missing.version_range
            );
        }
        for mismatch in &plan.version_mismatches {
            println!(
                "  VERSION {} needs {} {}{} but found {} [side {}]",
                mismatch.owner,
                mismatch.dependency,
                mismatch.required,
                if mismatch.mandatory {
                    " (required)"
                } else {
                    " (optional)"
                },
                mismatch.found,
                mismatch.side.as_str()
            );
        }
        for conflict in &plan.namespace_conflicts {
            println!("  NAMESPACE {} claimed by {:?}", conflict.0, conflict.1);
        }
        for cycle in &plan.cycles {
            println!(
                "  CYCLE {}",
                cycle
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            );
        }
        for error in &errors {
            println!(
                "  ERROR {}: {}",
                error.get("path").and_then(Value::as_str).unwrap_or("?"),
                error.get("error").and_then(Value::as_str).unwrap_or("?")
            );
        }
    }
    Ok(if errors.is_empty() { 0 } else { 1 })
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn run(args: RunArgs, json: bool) -> Result<u8> {
    let discovery = discover_jars(&args.paths)?;
    let mut inspections = Vec::new();
    let mut skipped = Vec::new();
    for jar in &discovery.jars {
        match inspect_jar(jar, None) {
            Ok(inspection) => inspections.push(inspection),
            Err(error) => skipped.push(json!({
                "path": jar,
                "verdict": "error",
                "reason": error.to_string(),
            })),
        }
    }

    let mut selected: Vec<PathBuf> = Vec::new();
    for inspection in &inspections {
        let runnable = inspection.report.verdict == Verdict::RunnableSubset;
        if runnable || args.include_blocked {
            selected.push(inspection.path.clone());
        } else {
            let reason = inspection
                .report
                .errors()
                .map(|blocker| blocker.message.clone())
                .collect::<Vec<_>>();
            skipped.push(json!({
                "path": inspection.path,
                "verdict": verdict_label(inspection.report.verdict),
                "reason": if reason.is_empty() { vec!["not runnable by Vibe's Forge API subset".to_string()] } else { reason },
            }));
        }
    }

    if selected.is_empty() {
        let report = json!({
            "started": false,
            "reason": "no jar is runnable by Vibe's Forge API subset; inspect the blockers or pass --include-blocked for diagnostics",
            "jars": inspections.iter().map(|inspection| inspection.summary()).collect::<Vec<_>>(),
            "skipped": skipped,
        });
        if json {
            print_json(&report)?;
        } else {
            println!("Vibe did not start the runtime: no jar is runnable.");
            for entry in &skipped {
                println!(
                    "  SKIP {} [{}]",
                    entry.get("path").and_then(Value::as_str).unwrap_or("?"),
                    entry.get("verdict").and_then(Value::as_str).unwrap_or("?")
                );
                if let Some(reasons) = entry.get("reason").and_then(Value::as_array) {
                    for reason in reasons {
                        println!("       {}", reason.as_str().unwrap_or("?"));
                    }
                }
            }
            println!("\nUse `vibe inspect <jar>` for details, or `vibe horizon export <jar>` to carry only assets/data.");
        }
        return Ok(3);
    }

    let java = locate_java(args.java.as_deref())?;
    let runtime = match resolve_runtime(args.runtime) {
        Some(path) if path.exists() => path,
        Some(path) => {
            return Err(VibeError::Runtime(format!(
                "runtime {} does not exist; build java/ or pass --runtime",
                path.display()
            )))
        }
        None => {
            return Err(VibeError::Runtime(
                "no Vibe runtime found; build java/ or pass --runtime".into(),
            ))
        }
    };

    let side = args.side.to_uppercase();
    let mut options = RuntimeOptions::new(java, runtime);
    options.side = side.clone();
    options.max_heap_mb = args.heap_mb;
    options.start_timeout = Duration::from_millis(args.timeout_ms.max(1_000));
    options.request_timeout = Duration::from_millis(args.timeout_ms.max(1_000));

    let mut client = RuntimeClient::spawn(options)?;
    let hello = client.hello().clone();
    let session = client.session().to_string();

    let mut info = None;
    let mut load = None;
    let mut lifecycle = None;
    let mut status = None;
    let mut registry = None;
    let outcome: Result<()> = (|| {
        info = Some(client.runtime_info()?);
        load = Some(client.load_mods(&selected, &side)?);
        lifecycle = Some(client.start_lifecycle()?);
        status = Some(client.mod_status()?);
        registry = Some(client.registry_snapshot()?);
        Ok(())
    })();
    let events = client.take_events();
    let shutdown_error = client.shutdown().err().map(|error| error.to_string());

    let lifecycle_ok = lifecycle
        .as_ref()
        .and_then(|value| value.get("ok"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let report = json!({
        "started": true,
        "session": session,
        "side": side,
        "hello": hello,
        "info": info,
        "load": load,
        "lifecycle": lifecycle,
        "status": status,
        "registry": registry,
        "events": events,
        "skipped": skipped,
        "error": outcome.as_ref().err().map(ToString::to_string),
        "shutdown_error": shutdown_error,
        "ok": lifecycle_ok && outcome.is_ok(),
    });

    if json {
        print_json(&report)?;
    } else {
        println!(
            "Vibe runtime: {} (Java {})",
            hello.runtime, hello.java_version
        );
        println!("  session: {}", report["session"].as_str().unwrap_or("?"));
        println!("  side:    {side}");
        if let Some(info) = &info {
            println!(
                "  runtime java: {}",
                info.get("java")
                    .and_then(|java| java.get("version"))
                    .and_then(Value::as_str)
                    .unwrap_or("?")
            );
        }
        if let Some(load) = &load {
            let mods = load
                .get("mods")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            println!("  loaded {} mod entrypoint(s):", mods.len());
            for module in &mods {
                println!(
                    "    {:<20} {:<14} {}",
                    module.get("id").and_then(Value::as_str).unwrap_or("?"),
                    module.get("status").and_then(Value::as_str).unwrap_or("?"),
                    module
                        .get("entrypoint")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                );
            }
            if let Some(failures) = load.get("failures").and_then(Value::as_array) {
                for failure in failures {
                    println!("    FAIL {}", failure.as_str().unwrap_or("?"));
                }
            }
        }
        if let Some(lifecycle) = &lifecycle {
            println!(
                "  lifecycle: phase={}, ok={}",
                lifecycle
                    .get("phase")
                    .and_then(Value::as_str)
                    .unwrap_or("?"),
                lifecycle
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            );
            if let Some(records) = lifecycle.get("events").and_then(Value::as_array) {
                let ok = records
                    .iter()
                    .filter(|record| record.get("ok").and_then(Value::as_bool).unwrap_or(false))
                    .count();
                println!("  event records: {ok}/{} succeeded", records.len());
                for record in records.iter().take(25) {
                    println!(
                        "    {:<32} {}",
                        record.get("event").and_then(Value::as_str).unwrap_or("?"),
                        record
                            .get("listener")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                    );
                }
            }
            if let Some(failures) = lifecycle.get("failures").and_then(Value::as_array) {
                for failure in failures {
                    println!("    FAIL {}", failure.as_str().unwrap_or("?"));
                }
            }
        }
        if let Some(registry) = &registry {
            println!(
                "  registry: {}",
                serde_json::to_string(registry).unwrap_or_default()
            );
        }
        if !events.is_empty() {
            println!("  bridge events: {}", events.len());
        }
        for entry in &skipped {
            println!(
                "  SKIP {} [{}]",
                entry.get("path").and_then(Value::as_str).unwrap_or("?"),
                entry.get("verdict").and_then(Value::as_str).unwrap_or("?")
            );
        }
        if let Err(error) = &outcome {
            println!("\nRuntime request failed: {error}");
        }
        if let Some(error) = &shutdown_error {
            println!("Shutdown warning: {error}");
        }
    }

    if outcome.is_err() {
        return Ok(4);
    }
    Ok(if lifecycle_ok { 0 } else { 4 })
}

// ---------------------------------------------------------------------------
// horizon
// ---------------------------------------------------------------------------

fn horizon_probe(args: ProbeArgs, json: bool) -> Result<u8> {
    let result = probe(args.ctl.as_deref());
    if json {
        print_json(&result)?;
    } else {
        match &result.ctl {
            Some(ctl) => println!("horizon-ctl: {}", ctl.display()),
            None => println!("horizon-ctl: not found"),
        }
        if let Some(summary) = &result.help_summary {
            println!("  {}", summary);
        }
        println!(
            "  game: {}",
            if result.game_reachable {
                "reachable"
            } else {
                "not reachable"
            }
        );
        for note in &result.notes {
            println!("  {note}");
        }
    }
    Ok(if result.ctl.is_some() { 0 } else { 1 })
}

fn horizon_export(args: ExportArgs, json: bool) -> Result<u8> {
    let options = ExportOptions {
        id: args.id,
        name: args.name,
        version: args.version,
        side: args.side,
        namespaces: args.namespaces,
        force: args.force,
        include_vanilla_tags: !args.no_vanilla_tags,
        include_convention_tags: !args.no_convention_tags,
        include_atlases: !args.no_atlases,
    };
    let exported = export_jar(&args.jar, &args.output, &options)?;
    if json {
        print_json(&exported)?;
    } else {
        println!("Wrote {}", exported.output.display());
        println!(
            "  id: {}  version: {}  side: {}",
            exported.id, exported.version, exported.side
        );
        println!(
            "  {} file(s), {} bytes; namespaces: {}",
            exported.files,
            exported.bytes,
            exported.namespaces.join(", ")
        );
        if !exported.outside_namespaces.is_empty() {
            println!(
                "  left out namespaces: {}",
                exported.outside_namespaces.join(", ")
            );
        }
        let mut skipped_reasons: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for entry in &exported.skipped {
            *skipped_reasons.entry(entry.reason.as_str()).or_default() += 1;
        }
        if !skipped_reasons.is_empty() {
            let summary: Vec<String> = skipped_reasons
                .iter()
                .map(|(reason, count)| format!("{reason} {count}"))
                .collect();
            println!(
                "  explained skips: {}{}",
                summary.join(", "),
                if exported.skipped_truncated {
                    " (list truncated; counts above are complete)"
                } else {
                    ""
                }
            );
            println!("  per-file reasons: see --json");
        }
        if !exported.warnings.is_empty() {
            println!("Warnings:");
            for warning in &exported.warnings {
                println!("  - {warning}");
            }
        }
        if !exported.notes.is_empty() {
            println!("Notes:");
            for note in &exported.notes {
                println!("  - {note}");
            }
        }
    }
    Ok(0)
}

fn horizon_validate(args: ValidateArgs, json: bool) -> Result<u8> {
    let report = validate_content_bundle(&args.bundle)?;
    if json {
        print_json(&report)?;
    } else {
        println!("Bundle: {}", report.path.display());
        println!("  sha256: {}", report.sha256);
        match (&report.id, &report.version) {
            (Some(id), Some(version)) => println!("  id: {id}  version: {version}"),
            _ => println!("  id/version: unavailable"),
        }
        if let Some(name) = &report.name {
            println!("  name: {name}");
        }
        println!(
            "  api: {}  side: {}",
            report.api.as_deref().unwrap_or("unavailable"),
            report.side.as_deref().unwrap_or("unavailable")
        );
        println!(
            "  {} file(s), {} bytes; {} block(s), {} item(s)",
            report.files, report.bytes, report.block_definitions, report.item_definitions
        );
        if !report.namespaces.is_empty() {
            println!("  namespaces: {}", report.namespaces.join(", "));
        }
        if !report.errors.is_empty() {
            println!("Errors:");
            for error in &report.errors {
                println!("  - {error}");
            }
        }
        if !report.warnings.is_empty() {
            println!("Warnings:");
            for warning in &report.warnings {
                println!("  - {warning}");
            }
        }
        println!(
            "Result: {}",
            if report.valid {
                "valid under the Horizon rules Vibe checks"
            } else {
                "invalid"
            }
        );
        if report.valid {
            println!(
                "  Vibe checks the documented manifest, namespace, path, size and block/item rules; run `cargo horizon-mod check` for Horizon's own validation."
            );
        }
    }
    Ok(if report.valid { 0 } else { 2 })
}

fn horizon_install(args: InstallArgs, json: bool) -> Result<u8> {
    let report = install_content_bundle(&args.bundle, &args.game_dir, args.force, args.dry_run)?;
    if json {
        print_json(&report)?;
    } else if report.dry_run {
        println!("Would install {}", report.bundle.display());
        println!("  to: {}", report.destination.display());
        println!("  {} bytes, sha256 {}", report.bytes, report.sha256);
    } else {
        println!("Installed {}", report.bundle.display());
        println!("  to: {}", report.destination.display());
        println!("  {} bytes, sha256 {}", report.bytes, report.sha256);
    }
    if !report.validation.warnings.is_empty() {
        println!("Warnings:");
        for warning in &report.validation.warnings {
            println!("  - {warning}");
        }
    }
    Ok(0)
}

fn resolve_runtime(explicit: Option<PathBuf>) -> Option<PathBuf> {
    explicit
        .or_else(|| {
            std::env::var_os("VIBE_RUNTIME_JAR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| default_runtime_path(&repo_root()))
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

fn verdict_label(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::RunnableSubset => "runnable-subset",
        Verdict::ContentOnly => "content-only",
        Verdict::Blocked => "blocked",
        Verdict::Unknown => "unknown",
    }
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| VibeError::Protocol(format!("could not serialize JSON: {error}")))?;
    println!("{text}");
    Ok(())
}
