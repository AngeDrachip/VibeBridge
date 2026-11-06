use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A Forge mod id. Forge's documented constraint is `^[a-z][a-z0-9_]{1,63}$`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModId(String);

impl ModId {
    pub fn parse(value: impl Into<String>) -> crate::Result<Self> {
        let value = value.into();
        let valid = (2..=64).contains(&value.len())
            && value
                .bytes()
                .next()
                .map(|b| b.is_ascii_lowercase())
                .unwrap_or(false)
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !valid {
            return Err(crate::VibeError::InvalidModId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Normalize an arbitrary jar-derived name into something usable for
    /// reporting. This is *not* a valid Forge id and is only used when a
    /// archive has no usable metadata.
    pub fn sanitize_lossy(value: &str) -> Self {
        let mut out = String::new();
        for ch in value.chars() {
            let ch = ch.to_ascii_lowercase();
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' {
                out.push(ch);
            } else if !out.ends_with('_') {
                out.push('_');
            }
        }
        while out.len() < 2 {
            out.push('_');
        }
        out.truncate(64);
        if !out
            .bytes()
            .next()
            .map(|b| b.is_ascii_lowercase())
            .unwrap_or(false)
        {
            out.insert(0, 'm');
            out.truncate(64);
        }
        Self(out)
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ModId {
    type Err = crate::VibeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// A Minecraft-style `namespace:path` resource location.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceLocation {
    pub namespace: String,
    pub path: String,
}

impl ResourceLocation {
    pub fn parse(value: &str) -> crate::Result<Self> {
        let (namespace, path) = match value.split_once(':') {
            Some((namespace, path)) => (namespace, path),
            None => ("minecraft", value),
        };
        let valid_ns = !namespace.is_empty()
            && namespace.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
            });
        let valid_path = !path.is_empty()
            && path.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'_' | b'-' | b'.' | b'/')
            });
        if !valid_ns || !valid_path {
            return Err(crate::VibeError::InvalidResourceLocation(value.to_string()));
        }
        Ok(Self {
            namespace: namespace.to_string(),
            path: path.to_string(),
        })
    }
}

impl fmt::Display for ResourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_documented_forge_ids() {
        for id in ["examplemod", "create", "t_and_t", "a1"] {
            assert_eq!(ModId::parse(id).unwrap().as_str(), id);
        }
    }

    #[test]
    fn rejects_invalid_forge_ids() {
        for id in [
            "",
            "A",
            "Upper",
            "1abc",
            "has-dash",
            "has space",
            &"x".repeat(65),
        ] {
            assert!(ModId::parse(id).is_err(), "{id:?} should be rejected");
        }
    }

    #[test]
    fn parses_resource_locations() {
        let id = ResourceLocation::parse("minecraft:textures/block/stone.png").unwrap();
        assert_eq!(id.namespace, "minecraft");
        assert_eq!(id.path, "textures/block/stone.png");
        assert_eq!(
            ResourceLocation::parse("stone").unwrap().namespace,
            "minecraft"
        );
    }
}
