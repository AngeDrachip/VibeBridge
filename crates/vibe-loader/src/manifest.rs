use std::collections::BTreeMap;

/// A parsed JAR manifest with case-insensitive lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    attributes: BTreeMap<String, String>,
}

impl Manifest {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes
            .get(&key.to_ascii_lowercase())
            .map(String::as_str)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.attributes.contains_key(&key.to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.attributes.iter()
    }

    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    pub fn split_values(&self, key: &str) -> Vec<String> {
        self.get(key)
            .map(|value| {
                value
                    .split(',')
                    .flat_map(|part| part.split_whitespace())
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Parse the main section of a JAR manifest, including continuation lines.
pub fn parse_manifest(text: &str) -> Manifest {
    let mut attributes: BTreeMap<String, String> = BTreeMap::new();
    let mut current_key: Option<String> = None;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix(' ') {
            if let Some(key) = &current_key {
                if let Some(value) = attributes.get_mut(key) {
                    value.push_str(rest);
                }
            }
            continue;
        }
        if line.is_empty() {
            current_key = None;
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            attributes.insert(key.clone(), value.trim().to_string());
            current_key = Some(key);
        }
    }

    Manifest { attributes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_continuations_and_case_insensitive_lookup() {
        let manifest = parse_manifest(
            "Manifest-Version: 1.0\r\nImplementation-Version: 1.2.3\r\nMixinConfigs: a.mixins.json,\r\n b.mixins.json\r\n",
        );
        assert_eq!(manifest.get("implementation-version"), Some("1.2.3"));
        assert_eq!(
            manifest.split_values("MixinConfigs"),
            vec!["a.mixins.json", "b.mixins.json"]
        );
    }
}
