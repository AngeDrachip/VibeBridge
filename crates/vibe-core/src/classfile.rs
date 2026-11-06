//! A tiny, dependency-free Java class-file reader.
//!
//! Vibe only needs a few facts from each class: its name, class-file version,
//! class-level Forge annotations, and which classes it references. Reading the
//! constant pool directly keeps `vibe inspect` fast and avoids executing mod
//! code while analysing it.

use std::collections::BTreeSet;

use crate::error::{Result, VibeError};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModAnnotation {
    pub value: Option<String>,
    pub modid: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventBusSubscriberAnnotation {
    pub bus: Option<String>,
    pub modid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFileFacts {
    pub internal_name: String,
    pub name: String,
    pub major_version: u16,
    pub java_release: u16,
    pub super_class: Option<String>,
    pub interfaces: Vec<String>,
    pub mod_annotation: Option<ModAnnotation>,
    pub event_bus_subscriber: Option<EventBusSubscriberAnnotation>,
    pub referenced_classes: BTreeSet<String>,
}

impl ClassFileFacts {
    pub fn references_minecraft(&self) -> bool {
        self.referenced_classes.iter().any(|class| {
            class.starts_with("net.minecraft.") && !class.starts_with("net.minecraftforge.")
        })
    }

    pub fn referenced_minecraft_classes(&self) -> impl Iterator<Item = &String> {
        self.referenced_classes.iter().filter(|class| {
            class.starts_with("net.minecraft.") && !class.starts_with("net.minecraftforge.")
        })
    }

    pub fn references_forge(&self) -> impl Iterator<Item = &String> {
        self.referenced_classes
            .iter()
            .filter(|class| class.starts_with("net.minecraftforge."))
    }
}

#[derive(Debug, Clone)]
enum CpEntry {
    Utf8(String),
    Class(u16),
    Other,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn need(&self, n: usize) -> Result<()> {
        if self.pos + n > self.bytes.len() {
            Err(VibeError::ClassFile(format!(
                "unexpected end of class file at byte {} (needed {n} more)",
                self.pos
            )))
        } else {
            Ok(())
        }
    }

    fn u1(&mut self) -> Result<u8> {
        self.need(1)?;
        let v = self.bytes[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn u2(&mut self) -> Result<u16> {
        self.need(2)?;
        let v = u16::from_be_bytes([self.bytes[self.pos], self.bytes[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn u4(&mut self) -> Result<u32> {
        self.need(4)?;
        let v = u32::from_be_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
            self.bytes[self.pos + 2],
            self.bytes[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.need(n)?;
        self.pos += n;
        Ok(())
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
}

/// Parse a `.class` file from raw bytes.
pub fn parse_class(bytes: &[u8]) -> Result<ClassFileFacts> {
    let mut r = Reader::new(bytes);
    if r.u4()? != 0xCAFE_BABE {
        return Err(VibeError::ClassFile(
            "bad magic (not a Java class file)".into(),
        ));
    }
    let _minor = r.u2()?;
    let major_version = r.u2()?;
    let cp_count = r.u2()? as usize;
    let mut cp: Vec<Option<CpEntry>> = vec![None; cp_count.max(1)];

    let mut i = 1;
    while i < cp_count {
        let tag = r.u1()?;
        let entry = match tag {
            1 => {
                let len = r.u2()? as usize;
                let raw = r.bytes(len)?;
                // Modified UTF-8 in class files is close enough to UTF-8 for
                // class names and descriptors; invalid sequences are preserved
                // lossily and only used for diagnostics.
                Some(CpEntry::Utf8(String::from_utf8_lossy(raw).into_owned()))
            }
            7 => Some(CpEntry::Class(r.u2()?)),
            3 | 4 => {
                r.skip(4)?;
                Some(CpEntry::Other)
            }
            5 | 6 => {
                r.skip(8)?;
                i += 1;
                Some(CpEntry::Other)
            }
            8 | 16 | 19 | 20 => {
                r.skip(2)?;
                Some(CpEntry::Other)
            }
            9 | 10 | 11 | 12 | 17 | 18 => {
                r.skip(4)?;
                Some(CpEntry::Other)
            }
            15 => {
                r.skip(3)?;
                Some(CpEntry::Other)
            }
            other => {
                return Err(VibeError::ClassFile(format!(
                    "unknown constant-pool tag {other} at index {i}"
                )))
            }
        };
        if i < cp.len() {
            cp[i] = entry;
        }
        i += 1;
    }

    let utf8 = |index: u16| -> Option<String> {
        match cp.get(index as usize).and_then(Option::as_ref) {
            Some(CpEntry::Utf8(s)) => Some(s.clone()),
            _ => None,
        }
    };
    let class_name = |index: u16| -> Option<String> {
        match cp.get(index as usize).and_then(Option::as_ref) {
            Some(CpEntry::Class(name_index)) => utf8(*name_index),
            _ => None,
        }
    };
    let dotted = |internal: String| internal.replace('/', ".");

    let _access = r.u2()?;
    let this_class = r.u2()?;
    let super_class = r.u2()?;
    let interface_count = r.u2()? as usize;
    let mut interface_indexes = Vec::with_capacity(interface_count);
    for _ in 0..interface_count {
        interface_indexes.push(r.u2()?);
    }

    let internal_name = class_name(this_class)
        .ok_or_else(|| VibeError::ClassFile("this_class does not name a UTF-8 entry".into()))?;
    let name = dotted(internal_name.clone());
    let super_class = if super_class == 0 {
        None
    } else {
        class_name(super_class).map(dotted)
    };
    let interfaces: Vec<String> = interface_indexes
        .into_iter()
        .filter_map(class_name)
        .map(dotted)
        .collect();

    let mut class_attributes: Vec<(String, Vec<u8>)> = Vec::new();
    for _ in 0..2 {
        // fields, then methods
        let count = r.u2()? as usize;
        for _ in 0..count {
            let _member_access = r.u2()?;
            let _name_index = r.u2()?;
            let _desc_index = r.u2()?;
            let attr_count = r.u2()? as usize;
            for _ in 0..attr_count {
                let _attr_name = utf8(r.u2()?).unwrap_or_default();
                let len = r.u4()? as usize;
                let _ = r.bytes(len)?;
            }
        }
    }
    let class_attr_count = r.u2()? as usize;
    for _ in 0..class_attr_count {
        let attr_name = utf8(r.u2()?).unwrap_or_default();
        let len = r.u4()? as usize;
        let data = r.bytes(len)?.to_vec();
        class_attributes.push((attr_name, data));
    }

    let mut mod_annotation = None;
    let mut event_bus_subscriber = None;
    for (attr_name, data) in class_attributes {
        if attr_name != "RuntimeVisibleAnnotations" && attr_name != "RuntimeInvisibleAnnotations" {
            continue;
        }
        let mut ar = Reader::new(&data);
        let count = match ar.u2() {
            Ok(count) => count,
            Err(_) => continue,
        };
        for _ in 0..count {
            let Ok(annotation) = read_annotation(&mut ar, &utf8) else {
                break;
            };
            match annotation.descriptor.as_str() {
                "Lnet/minecraftforge/fml/common/Mod;" => {
                    mod_annotation = Some(ModAnnotation {
                        value: annotation.string("value"),
                        modid: annotation.string("modid"),
                        name: annotation.string("name"),
                        version: annotation.string("version"),
                    });
                }
                "Lnet/minecraftforge/fml/common/Mod$EventBusSubscriber;" => {
                    event_bus_subscriber = Some(EventBusSubscriberAnnotation {
                        bus: annotation.enum_value("bus"),
                        modid: annotation.string("modid"),
                    });
                }
                _ => {}
            }
        }
    }

    let mut referenced_classes = BTreeSet::new();
    for entry in cp.iter().flatten() {
        if let CpEntry::Class(index) = entry {
            if let Some(internal) = utf8(*index) {
                if !internal.is_empty() && !internal.starts_with('[') {
                    referenced_classes.insert(dotted(internal));
                }
            }
        }
    }
    // Field and method descriptors also name classes, but those names are
    // Utf8 entries, not Class entries. Extract only descriptor-shaped names to
    // avoid treating arbitrary strings as class references.
    for entry in cp.iter().flatten() {
        if let CpEntry::Utf8(text) = entry {
            collect_descriptor_classes(text, &mut referenced_classes);
        }
    }
    referenced_classes.remove(&name);

    Ok(ClassFileFacts {
        internal_name,
        name,
        major_version,
        java_release: major_version.saturating_sub(44),
        super_class,
        interfaces,
        mod_annotation,
        event_bus_subscriber,
        referenced_classes,
    })
}

fn collect_descriptor_classes(text: &str, out: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'L' && i + 1 < bytes.len() {
            let rest = &text[i + 1..];
            if rest.starts_with("net/minecraft/") || rest.starts_with("net/minecraftforge/") {
                let end = rest
                    .find(|c: char| {
                        !(c.is_ascii_alphanumeric() || matches!(c, '/' | '$' | '_' | '.'))
                    })
                    .unwrap_or(rest.len());
                let class = rest[..end].replace('/', ".");
                if class.contains('.') {
                    out.insert(class);
                }
                i += 1 + end;
                continue;
            }
        }
        i += 1;
    }
}

struct Annotation {
    descriptor: String,
    values: Vec<(String, ElementValue)>,
}

enum ElementValue {
    String(String),
    Enum { name: String },
    Other,
}

impl Annotation {
    fn string(&self, key: &str) -> Option<String> {
        self.values.iter().find_map(|(name, value)| {
            if name == key {
                if let ElementValue::String(value) = value {
                    return Some(value.clone());
                }
            }
            None
        })
    }

    fn enum_value(&self, key: &str) -> Option<String> {
        self.values.iter().find_map(|(name, value)| {
            if name == key {
                if let ElementValue::Enum { name } = value {
                    return Some(name.clone());
                }
            }
            None
        })
    }
}

fn read_annotation(
    r: &mut Reader<'_>,
    utf8: &impl Fn(u16) -> Option<String>,
) -> Result<Annotation> {
    let descriptor =
        utf8(r.u2()?).ok_or_else(|| VibeError::ClassFile("annotation type is not UTF-8".into()))?;
    let pairs = r.u2()? as usize;
    let mut values = Vec::with_capacity(pairs);
    for _ in 0..pairs {
        let name = utf8(r.u2()?)
            .ok_or_else(|| VibeError::ClassFile("annotation element name is not UTF-8".into()))?;
        let value = read_element_value(r, utf8)?;
        values.push((name, value));
    }
    Ok(Annotation { descriptor, values })
}

fn read_element_value(
    r: &mut Reader<'_>,
    utf8: &impl Fn(u16) -> Option<String>,
) -> Result<ElementValue> {
    let tag = r.u1()?;
    match tag {
        b's' => Ok(ElementValue::String(utf8(r.u2()?).unwrap_or_default())),
        b'e' => {
            let _type_name = r.u2()?;
            let const_name = utf8(r.u2()?).unwrap_or_default();
            Ok(ElementValue::Enum { name: const_name })
        }
        b'c' => {
            let _ = r.u2()?;
            Ok(ElementValue::Other)
        }
        b'@' => {
            let _ = read_annotation(r, utf8)?;
            Ok(ElementValue::Other)
        }
        b'[' => {
            let count = r.u2()? as usize;
            for _ in 0..count {
                let _ = read_element_value(r, utf8)?;
            }
            Ok(ElementValue::Other)
        }
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => {
            let _ = r.u2()?;
            Ok(ElementValue::Other)
        }
        other => Err(VibeError::ClassFile(format!(
            "unknown annotation element tag {:?}",
            other as char
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_class_bytes() {
        assert!(parse_class(b"not a class").is_err());
    }

    #[test]
    fn descriptor_scanning_finds_nested_types() {
        let mut out = BTreeSet::new();
        collect_descriptor_classes(
            "(Lnet/minecraft/world/level/Level;Lnet/minecraftforge/eventbus/api/Event;)V",
            &mut out,
        );
        assert!(out.contains("net.minecraft.world.level.Level"));
        assert!(out.contains("net.minecraftforge.eventbus.api.Event"));
    }
}
