use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::classfile::ClassFileFacts;
use crate::metadata::LoaderKind;

/// Severity of a compatibility finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockerKind {
    MinecraftClasses,
    UnsupportedForgeApi,
    Coremod,
    MixinTransformer,
    AccessTransformer,
    ExternalClasses,
    NestedJars,
    NativeLibraries,
    MissingEntrypoint,
    UnsupportedLoader,
    JavaVersionTooNew,
    JavaVersionTooOld,
    ClassLoadError,
    NoForgeMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub kind: BlockerKind,
    pub severity: Severity,
    pub message: String,
    /// A bounded sample of the evidence, at most 32 entries.
    pub evidence: Vec<String>,
    /// How many evidence entries were found in total.
    #[serde(default)]
    pub evidence_total: usize,
    /// Evidence grouped by package or path prefix, so a large jar stays
    /// readable. Counts describe the full evidence, not the sample.
    #[serde(default)]
    pub evidence_groups: Vec<EvidenceGroup>,
    /// Whether more groups existed than are listed here.
    #[serde(default)]
    pub evidence_groups_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceGroup {
    pub label: String,
    pub count: usize,
    pub examples: Vec<String>,
}

const MAX_EVIDENCE: usize = 32;
const MAX_EVIDENCE_GROUPS: usize = 24;
const MAX_GROUP_EXAMPLES: usize = 3;

impl Blocker {
    fn new(kind: BlockerKind, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            kind,
            severity,
            message: message.into(),
            evidence: Vec::new(),
            evidence_total: 0,
            evidence_groups: Vec::new(),
            evidence_groups_truncated: false,
        }
    }

    fn with_evidence(mut self, evidence: impl IntoIterator<Item = String>) -> Self {
        let all: Vec<String> = evidence.into_iter().collect();
        self.evidence_total = all.len();
        let (mut groups, truncated) = group_evidence(&all);
        groups.truncate(MAX_EVIDENCE_GROUPS);
        self.evidence_groups = groups;
        self.evidence_groups_truncated = truncated;
        self.evidence = all.into_iter().take(MAX_EVIDENCE).collect();
        self
    }
}

/// Group class names by package and other evidence by path prefix.
///
/// Class names use their full package (`net.minecraft.world.level` for
/// `net.minecraft.world.level.Level`); paths use the first two segments.
/// Groups are sorted by descending size, then label, so the largest category
/// appears first.
fn group_evidence(evidence: &[String]) -> (Vec<EvidenceGroup>, bool) {
    let mut groups: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for item in evidence {
        let label = evidence_label(item);
        let entry = groups.entry(label).or_insert((0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < MAX_GROUP_EXAMPLES {
            entry.1.push(item.clone());
        }
    }
    let mut grouped: Vec<EvidenceGroup> = groups
        .into_iter()
        .map(|(label, (count, examples))| EvidenceGroup {
            label,
            count,
            examples,
        })
        .collect();
    grouped.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    let truncated = grouped.len() > MAX_EVIDENCE_GROUPS;
    (grouped, truncated)
}

fn evidence_label(item: &str) -> String {
    if item.contains('/') || item.contains('\\') {
        let normalized = item.replace('\\', "/");
        return normalized.split('/').take(2).collect::<Vec<_>>().join("/");
    }
    if is_class_name(item) {
        let segments: Vec<&str> = item.split('.').collect();
        let package = segments.len().saturating_sub(1).max(1);
        return segments[..package].join(".");
    }
    item.to_string()
}

fn is_class_name(value: &str) -> bool {
    let mut segments = 0;
    for segment in value.split('.') {
        segments += 1;
        if segment.is_empty()
            || !segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$'))
        {
            return false;
        }
    }
    segments >= 2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The Java runtime can execute this mod's entrypoint against Vibe's
    /// Forge API subset. This is *not* a statement of general Forge
    /// compatibility.
    RunnableSubset,
    /// Metadata/content is usable but there is no Java code to execute, or the
    /// mod's code is intentionally data-only.
    ContentOnly,
    /// At least one hard blocker prevents the mod from running.
    Blocked,
    /// The archive is not recognisably a Forge mod.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub verdict: Verdict,
    pub blockers: Vec<Blocker>,
    pub entrypoints: Vec<String>,
    pub declared_mod_ids: Vec<String>,
    pub referenced_minecraft_classes: Vec<String>,
    pub unsupported_forge_api: Vec<String>,
    /// Classes referenced by the jar that neither the jar, the JDK, nor Vibe
    /// provides. They are almost always a third-party library the mod expects
    /// to find at runtime.
    pub external_classes: Vec<String>,
    /// External classes grouped by their first two package segments.
    pub external_libraries: BTreeMap<String, usize>,
    pub class_count: usize,
    pub max_java_release: Option<u16>,
    pub has_assets: bool,
    pub has_data: bool,
}

impl CompatibilityReport {
    pub fn errors(&self) -> impl Iterator<Item = &Blocker> {
        self.blockers
            .iter()
            .filter(|blocker| blocker.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Blocker> {
        self.blockers
            .iter()
            .filter(|blocker| blocker.severity == Severity::Warning)
    }

    pub fn is_runnable_subset(&self) -> bool {
        self.verdict == Verdict::RunnableSubset
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnalysisFacts {
    pub loader: Option<LoaderKind>,
    pub classes: Vec<ClassFileFacts>,
    /// Every class name the jar ships, including classes that failed to parse.
    pub provided_classes: BTreeSet<String>,
    pub class_errors: Vec<String>,
    pub nested_jars: Vec<String>,
    pub mixin_configs: Vec<String>,
    pub access_transformers: Vec<String>,
    pub core_plugin: Option<String>,
    pub native_libraries: Vec<String>,
    pub has_assets: bool,
    pub has_data: bool,
    pub runtime_java_release: Option<u16>,
}

/// The Forge API surface implemented by Vibe's clean-room Java shim.
///
/// This list is intentionally explicit. A mod that references any other
/// `net.minecraftforge.*` class is reported as blocked rather than silently
/// failing at class-load time.
pub const SUPPORTED_FORGE_API: &[&str] = &[
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
    "net.minecraftforge.fml.javafmlmod.FMLJavaModLoadingContext",
    "net.minecraftforge.registries.DeferredRegister",
    "net.minecraftforge.registries.ForgeRegistries",
    "net.minecraftforge.registries.IForgeRegistry",
    "net.minecraftforge.registries.RegisterEvent",
    "net.minecraftforge.registries.RegisterEvent$RegisterHelper",
    "net.minecraftforge.registries.RegistryObject",
    "net.minecraftforge.registries.VibeRegistry",
    "net.minecraftforge.registries.VibeRegistryManager",
];

/// The Minecraft API surface implemented by Vibe's clean-room shim.
///
/// Minecraft classes are only runnable when Vibe provides a real
/// implementation. This list must stay in sync with
/// `java/vibe-forge-api/src/main/java/net/minecraft/`; anything else is
/// reported as a hard blocker instead of failing later during class loading.
pub const SUPPORTED_MINECRAFT_API: &[&str] = &[
    "net.minecraft.core.Registry",
    "net.minecraft.core.registries.Registries",
    "net.minecraft.network.chat.Component",
    "net.minecraft.resources.ResourceKey",
    "net.minecraft.resources.ResourceLocation",
    "net.minecraft.world.item.CreativeModeTab",
    "net.minecraft.world.item.CreativeModeTab$Builder",
    "net.minecraft.world.item.Item",
    "net.minecraft.world.item.Item$Properties",
    "net.minecraft.world.item.ItemStack",
    "net.minecraft.world.level.block.Block",
    "net.minecraft.world.level.block.Block$Properties",
    "net.minecraft.world.level.block.state.BlockBehaviour",
    "net.minecraft.world.level.block.state.BlockBehaviour$Properties",
];

pub fn analyze(facts: &AnalysisFacts) -> CompatibilityReport {
    let mut blockers = Vec::new();
    let mut referenced_minecraft = BTreeSet::new();
    let mut referenced_forge = BTreeSet::new();
    let mut entrypoints = Vec::new();
    let mut declared_mod_ids = Vec::new();
    let mut max_java_release = None;

    for class in &facts.classes {
        max_java_release = Some(max_java_release.unwrap_or(0).max(class.java_release));
        if let Some(annotation) = &class.mod_annotation {
            entrypoints.push(class.name.clone());
            if let Some(id) = annotation
                .value
                .as_ref()
                .or(annotation.modid.as_ref())
                .filter(|id| !id.is_empty())
            {
                declared_mod_ids.push(id.clone());
            }
        }
        referenced_minecraft.extend(class.referenced_minecraft_classes().cloned());
        referenced_forge.extend(class.references_forge().cloned());
    }

    if !referenced_minecraft.is_empty() {
        let supported_minecraft: BTreeSet<&str> = SUPPORTED_MINECRAFT_API.iter().copied().collect();
        let unsupported_minecraft: Vec<String> = referenced_minecraft
            .iter()
            .filter(|class| !supported_minecraft.contains(class.as_str()))
            .cloned()
            .collect();
        if !unsupported_minecraft.is_empty() {
            let count = unsupported_minecraft.len();
            blockers.push(
                Blocker::new(
                    BlockerKind::MinecraftClasses,
                    Severity::Error,
                    format!(
                        "references {count} Minecraft Java class{} that Vibe's API subset does not provide",
                        if count == 1 { "" } else { "es" }
                    ),
                )
                .with_evidence(unsupported_minecraft),
            );
        }
    }

    let supported: BTreeSet<&str> = SUPPORTED_FORGE_API.iter().copied().collect();
    let unsupported: Vec<String> = referenced_forge
        .iter()
        .filter(|class| !supported.contains(class.as_str()))
        .cloned()
        .collect();
    if !unsupported.is_empty() {
        blockers.push(
            Blocker::new(
                BlockerKind::UnsupportedForgeApi,
                Severity::Error,
                format!(
                    "uses {} Forge API class{} outside Vibe's implemented subset",
                    unsupported.len(),
                    if unsupported.len() == 1 { "" } else { "es" }
                ),
            )
            .with_evidence(unsupported.clone()),
        );
    }

    let mut provided: BTreeSet<&str> = facts.provided_classes.iter().map(String::as_str).collect();
    provided.extend(facts.classes.iter().map(|class| class.name.as_str()));

    let mut external = BTreeSet::new();
    for class in &facts.classes {
        for reference in &class.referenced_classes {
            if is_external_class(reference, &provided) {
                external.insert(reference.clone());
            }
        }
    }
    let external_libraries = library_counts(&external);
    if !external.is_empty() {
        let libraries: Vec<&str> = external_libraries.keys().map(String::as_str).collect();
        blockers.push(
            Blocker::new(
                BlockerKind::ExternalClasses,
                Severity::Error,
                format!(
                    "references {} class{} from {} librar{} the jar does not bundle and Vibe does not provide: {}",
                    external.len(),
                    if external.len() == 1 { "" } else { "es" },
                    libraries.len(),
                    if libraries.len() == 1 { "y" } else { "ies" },
                    libraries.join(", ")
                ),
            )
            .with_evidence(external.iter().cloned()),
        );
    }

    if !facts.nested_jars.is_empty() {
        blockers.push(
            Blocker::new(
                BlockerKind::NestedJars,
                Severity::Warning,
                "contains nested jars (Jar-in-Jar); Vibe extracts them into the mod class loader, but nested dependency version ranges are not resolved",
            )
            .with_evidence(facts.nested_jars.clone()),
        );
    }
    if !facts.mixin_configs.is_empty() {
        blockers.push(
            Blocker::new(
                BlockerKind::MixinTransformer,
                Severity::Error,
                "uses Mixin bytecode transformation, which Vibe does not implement",
            )
            .with_evidence(facts.mixin_configs.clone()),
        );
    }
    if !facts.access_transformers.is_empty() {
        blockers.push(
            Blocker::new(
                BlockerKind::AccessTransformer,
                Severity::Error,
                "uses Forge access transformers, which Vibe does not apply",
            )
            .with_evidence(facts.access_transformers.clone()),
        );
    }
    if let Some(core_plugin) = &facts.core_plugin {
        blockers.push(
            Blocker::new(
                BlockerKind::Coremod,
                Severity::Error,
                "declares an FML core plugin; Vibe does not implement coremods",
            )
            .with_evidence([core_plugin.clone()]),
        );
    }
    if !facts.native_libraries.is_empty() {
        blockers.push(
            Blocker::new(
                BlockerKind::NativeLibraries,
                Severity::Warning,
                "contains native libraries; Vibe does not yet extract or load them",
            )
            .with_evidence(facts.native_libraries.clone()),
        );
    }
    if !facts.class_errors.is_empty() {
        blockers.push(
            Blocker::new(
                BlockerKind::ClassLoadError,
                Severity::Warning,
                "some class files could not be parsed and were skipped during analysis",
            )
            .with_evidence(facts.class_errors.clone()),
        );
    }

    let loader = facts
        .loader
        .clone()
        .unwrap_or(LoaderKind::Unknown("none".into()));
    let java_entrypoint_required = loader.needs_java_entrypoint();
    if java_entrypoint_required && entrypoints.is_empty() {
        blockers.push(Blocker::new(
            BlockerKind::MissingEntrypoint,
            Severity::Error,
            "loader is javafml/legacy but no class carries @Mod",
        ));
    }
    if let LoaderKind::Unknown(name) = &loader {
        blockers.push(Blocker::new(
            BlockerKind::UnsupportedLoader,
            Severity::Error,
            format!("unsupported language loader {name:?}"),
        ));
    }

    if let Some(runtime) = facts.runtime_java_release {
        if let Some(max) = max_java_release {
            if max > runtime {
                blockers.push(Blocker::new(
                    BlockerKind::JavaVersionTooNew,
                    Severity::Error,
                    format!(
                        "classes target Java {max}, but the selected runtime is Java {runtime}"
                    ),
                ));
            } else if runtime < 17 {
                blockers.push(Blocker::new(
                    BlockerKind::JavaVersionTooOld,
                    Severity::Error,
                    "Vibe's Java runtime requires Java 17 or newer",
                ));
            }
        }
    }

    if facts.loader.is_none() {
        blockers.push(Blocker::new(
            BlockerKind::NoForgeMetadata,
            Severity::Warning,
            "no Forge metadata (mods.toml / mcmod.info) was found",
        ));
    }

    let has_error = blockers.iter().any(|b| b.severity == Severity::Error);
    let verdict = if has_error {
        Verdict::Blocked
    } else if matches!(loader, LoaderKind::LowCodeFml)
        || (entrypoints.is_empty() && facts.loader.is_some())
    {
        Verdict::ContentOnly
    } else if entrypoints.is_empty() {
        Verdict::Unknown
    } else {
        Verdict::RunnableSubset
    };

    blockers.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| blocker_rank(&a.kind).cmp(&blocker_rank(&b.kind)))
            .then_with(|| a.message.cmp(&b.message))
    });

    CompatibilityReport {
        verdict,
        blockers,
        entrypoints,
        declared_mod_ids: dedup(declared_mod_ids),
        referenced_minecraft_classes: referenced_minecraft.into_iter().collect(),
        unsupported_forge_api: unsupported,
        external_classes: external.into_iter().collect(),
        external_libraries,
        class_count: facts.classes.len(),
        max_java_release,
        has_assets: facts.has_assets,
        has_data: facts.has_data,
    }
}

/// Whether a referenced class must come from a library the jar does not ship.
///
/// The JDK, the Java platform modules, Vibe's Forge shim, log4j shim, and
/// compile-time-only annotation processors are never external. Minecraft and
/// unsupported Forge classes have their own, more specific blockers.
fn is_external_class(class: &str, provided: &BTreeSet<&str>) -> bool {
    if class.is_empty() || class.starts_with('[') || provided.contains(class) {
        return false;
    }
    // `net.minecraftforge.` is checked first: it also starts with `net.minecraft.`.
    if class.starts_with("net.minecraftforge.") {
        return false;
    }
    if class.starts_with("net.minecraft.") {
        return false;
    }
    const PROVIDED_PREFIXES: &[&str] = &[
        "java.",
        "javax.",
        "jdk.",
        "sun.",
        "com.sun.",
        "org.w3c.",
        "org.xml.",
        "org.ietf.",
        "org.apache.logging.log4j.",
    ];
    if PROVIDED_PREFIXES
        .iter()
        .any(|prefix| class.starts_with(prefix))
    {
        return false;
    }
    // Retention-policy annotations are compile-time metadata; their absence does
    // not prevent a class from loading. Filtering them avoids reporting a
    // missing library for an annotation the mod never calls.
    const COMPILE_ONLY_PREFIXES: &[&str] = &[
        "org.jetbrains.annotations.",
        "org.intellij.lang.annotations.",
        "org.checkerframework.",
        "com.google.errorprone.annotations.",
        "edu.umd.cs.findbugs.annotations.",
        "javax.annotation.",
        "jakarta.annotation.",
        "lombok.",
    ];
    !COMPILE_ONLY_PREFIXES
        .iter()
        .any(|prefix| class.starts_with(prefix))
}

fn library_counts(classes: &BTreeSet<String>) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for class in classes {
        let mut parts = class.split('.');
        let library = match (parts.next(), parts.next()) {
            (Some(first), Some(second)) => format!("{first}.{second}"),
            _ => class.clone(),
        };
        *counts.entry(library).or_default() += 1;
    }
    counts
}

fn blocker_rank(kind: &BlockerKind) -> u8 {
    match kind {
        BlockerKind::MinecraftClasses => 0,
        BlockerKind::Coremod => 1,
        BlockerKind::MixinTransformer => 2,
        BlockerKind::AccessTransformer => 3,
        BlockerKind::ExternalClasses => 4,
        BlockerKind::UnsupportedLoader => 5,
        BlockerKind::MissingEntrypoint => 6,
        BlockerKind::JavaVersionTooNew | BlockerKind::JavaVersionTooOld => 7,
        BlockerKind::UnsupportedForgeApi => 8,
        BlockerKind::NestedJars => 9,
        BlockerKind::NativeLibraries => 10,
        BlockerKind::ClassLoadError => 11,
        BlockerKind::NoForgeMetadata => 12,
    }
}

fn dedup(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

/// Convenience helper for reports that need the union of blocked mod ids.
pub fn blocked_mod_ids(reports: &BTreeMap<String, CompatibilityReport>) -> Vec<String> {
    reports
        .iter()
        .filter(|(_, report)| report.verdict == Verdict::Blocked)
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classfile::ClassFileFacts;

    fn class(name: &str, annotation: Option<&str>) -> ClassFileFacts {
        ClassFileFacts {
            internal_name: name.replace('.', "/"),
            name: name.to_string(),
            major_version: 61,
            java_release: 17,
            super_class: Some("java.lang.Object".into()),
            interfaces: Vec::new(),
            mod_annotation: annotation.map(|id| crate::classfile::ModAnnotation {
                value: Some(id.into()),
                ..Default::default()
            }),
            event_bus_subscriber: None,
            referenced_classes: BTreeSet::new(),
        }
    }

    #[test]
    fn runnable_subset_requires_no_hard_blockers() {
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![class("com.example.ExampleMod", Some("examplemod"))],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        let report = analyze(&facts);
        assert_eq!(report.verdict, Verdict::RunnableSubset);
        assert_eq!(report.entrypoints, vec!["com.example.ExampleMod"]);
    }

    #[test]
    fn minecraft_references_block_a_mod() {
        let mut entry = class("com.example.ExampleMod", Some("examplemod"));
        entry
            .referenced_classes
            .insert("net.minecraft.world.level.Level".into());
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![entry],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        let report = analyze(&facts);
        assert_eq!(report.verdict, Verdict::Blocked);
        assert!(report
            .blockers
            .iter()
            .any(|b| b.kind == BlockerKind::MinecraftClasses));
    }

    #[test]
    fn supported_minecraft_and_registry_api_does_not_block() {
        let mut entry = class("com.example.ExampleMod", Some("examplemod"));
        for supported in [
            "net.minecraft.world.item.Item",
            "net.minecraft.world.item.Item$Properties",
            "net.minecraft.world.level.block.Block",
            "net.minecraft.resources.ResourceLocation",
            "net.minecraftforge.registries.DeferredRegister",
            "net.minecraftforge.registries.RegistryObject",
            "net.minecraftforge.registries.RegisterEvent",
        ] {
            entry.referenced_classes.insert(supported.into());
        }
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![entry],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        let report = analyze(&facts);
        assert_eq!(report.verdict, Verdict::RunnableSubset);
        assert!(!report
            .blockers
            .iter()
            .any(|b| b.kind == BlockerKind::MinecraftClasses));
        assert!(!report
            .blockers
            .iter()
            .any(|b| b.kind == BlockerKind::UnsupportedForgeApi));
    }

    #[test]
    fn mixin_blocks_a_mod() {
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![class("com.example.ExampleMod", Some("examplemod"))],
            mixin_configs: vec!["example.mixins.json".into()],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        let report = analyze(&facts);
        assert_eq!(report.verdict, Verdict::Blocked);
    }

    #[test]
    fn lowcode_is_content_only() {
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::LowCodeFml),
            has_data: true,
            runtime_java_release: Some(17),
            ..Default::default()
        };
        assert_eq!(analyze(&facts).verdict, Verdict::ContentOnly);
    }

    #[test]
    fn missing_third_party_library_blocks_a_mod() {
        let mut entry = class("com.example.ExampleMod", Some("examplemod"));
        entry
            .referenced_classes
            .insert("com.google.gson.Gson".into());
        entry.referenced_classes.insert("java.util.List".into());
        entry
            .referenced_classes
            .insert("org.jetbrains.annotations.Nullable".into());
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![entry],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        let report = analyze(&facts);
        assert_eq!(report.verdict, Verdict::Blocked);
        assert_eq!(report.external_classes, vec!["com.google.gson.Gson"]);
        assert_eq!(report.external_libraries.get("com.google"), Some(&1));
        assert!(report
            .blockers
            .iter()
            .any(|blocker| blocker.kind == BlockerKind::ExternalClasses));
    }

    #[test]
    fn classes_shipped_with_the_mod_are_not_external() {
        let mut entry = class("com.example.ExampleMod", Some("examplemod"));
        entry.referenced_classes.insert("com.example.Helper".into());
        let mut facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![entry],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        facts.provided_classes.insert("com.example.Helper".into());
        let report = analyze(&facts);
        assert_eq!(report.verdict, Verdict::RunnableSubset);
        assert!(report.external_classes.is_empty());
    }

    #[test]
    fn large_evidence_is_grouped_by_package_and_capped() {
        let mut entry = class("com.example.ExampleMod", Some("examplemod"));
        for index in 0..50 {
            entry
                .referenced_classes
                .insert(format!("net.minecraft.world.level.Block{index}"));
        }
        for index in 0..30 {
            entry
                .referenced_classes
                .insert(format!("net.minecraft.client.gui.screens.Screen{index}"));
        }
        for index in 0..10 {
            entry
                .referenced_classes
                .insert(format!("net.minecraft.world.item.Item{index}"));
        }
        let facts = AnalysisFacts {
            loader: Some(LoaderKind::JavaFml),
            classes: vec![entry],
            runtime_java_release: Some(17),
            ..Default::default()
        };
        let report = analyze(&facts);
        let blocker = report
            .blockers
            .iter()
            .find(|blocker| blocker.kind == BlockerKind::MinecraftClasses)
            .expect("Minecraft blocker");

        assert_eq!(blocker.evidence_total, 90);
        assert_eq!(blocker.evidence.len(), 32);
        assert_eq!(
            blocker
                .evidence_groups
                .iter()
                .map(|group| group.count)
                .sum::<usize>(),
            90
        );
        assert_eq!(
            blocker.evidence_groups[0].label,
            "net.minecraft.world.level"
        );
        assert_eq!(blocker.evidence_groups[0].count, 50);
        assert_eq!(blocker.evidence_groups[1].count, 30);
        assert!(!blocker.evidence_groups_truncated);
    }

    #[test]
    fn too_many_groups_are_truncated_but_flagged() {
        let evidence: Vec<String> = (0..40)
            .map(|index| format!("library{index}.component.Type"))
            .collect();
        let (groups, truncated) = group_evidence(&evidence);
        assert_eq!(groups.len(), 40);
        assert!(truncated);
        assert_eq!(groups.iter().map(|group| group.count).sum::<usize>(), 40);
    }

    #[test]
    fn path_evidence_is_grouped_by_directory() {
        let evidence = vec![
            "META-INF/jarjar/first.jar".to_string(),
            "META-INF/jarjar/second.jar".to_string(),
            "META-INF/accesstransformer.cfg".to_string(),
        ];
        let (groups, _) = group_evidence(&evidence);
        assert!(groups
            .iter()
            .any(|group| group.label == "META-INF/jarjar" && group.count == 2));
    }
}
