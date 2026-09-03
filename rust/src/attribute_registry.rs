use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttributeRegistryEntry {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub display_group: Option<String>,
    #[serde(default)]
    pub display_level: Option<u8>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub units: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    #[serde(default)]
    pub deprecated_reason: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttributeRegistryConfig {
    #[serde(default)]
    pub attributes: HashMap<String, AttributeRegistryEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct AttributeRegistry {
    pub attributes: HashMap<String, AttributeRegistryEntry>,
}

impl AttributeRegistry {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: AttributeRegistryConfig = serde_yaml::from_str(&content)?;
        Ok(Self {
            attributes: config.attributes,
        })
    }

    pub fn load_default() -> Result<Self, Box<dyn std::error::Error>> {
        let mut candidates = vec![
            PathBuf::from("config/attributes/boat-attribute-registry.yaml"),
            PathBuf::from("rust/config/attributes/boat-attribute-registry.yaml"),
            PathBuf::from("../rust/config/attributes/boat-attribute-registry.yaml"),
        ];

        if let Ok(current_dir) = std::env::current_dir() {
            let mut dir = Some(current_dir);
            while let Some(path) = dir.take() {
                candidates.push(path.join("config/attributes/boat-attribute-registry.yaml"));
                candidates.push(path.join("rust/config/attributes/boat-attribute-registry.yaml"));
                dir = path.parent().map(Path::to_path_buf);
            }
        }

        for candidate in candidates {
            if candidate.exists() {
                return Self::load(candidate);
            }
        }

        Err("No default attribute registry found".into())
    }

    pub fn lookup(&self, key: &str) -> Option<&AttributeRegistryEntry> {
        self.attributes.get(key)
    }

    pub fn lookup_alias(&self, key: &str) -> Option<&AttributeRegistryEntry> {
        self.attributes
            .iter()
            .find_map(|(registered_key, entry)| {
                if registered_key == key || entry.aliases.iter().any(|alias| alias == key) {
                    Some((registered_key, entry))
                } else {
                    None
                }
            })
            .map(|(_, entry)| entry)
    }

    pub fn require_registered(&self, key: &str) -> Result<(), String> {
        if self.lookup(key).is_some() || self.lookup_alias(key).is_some() {
            Ok(())
        } else {
            Err(format!(
                "attribute '{key}' is not registered in the attribute registry"
            ))
        }
    }

    pub fn find_unmapped_keys<I, S>(&self, keys: I) -> Vec<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut missing = keys
            .into_iter()
            .map(|key| key.into())
            .filter(|key| self.lookup(key).is_none() && self.lookup_alias(key).is_none())
            .collect::<Vec<_>>();
        missing.sort();
        missing.dedup();
        missing
    }

    pub fn require_registered_keys<I, S>(&self, keys: I) -> Result<(), String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let missing = self.find_unmapped_keys(keys);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(format!("unregistered attributes: {}", missing.join(", ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads_core_feature_fields() {
        let registry = AttributeRegistry::load_default().unwrap();
        let feature_id = registry.lookup("feature_id").unwrap();
        assert_eq!(feature_id.display_group.as_deref(), Some("coordinates"));
        assert_eq!(feature_id.display_name.as_deref(), Some("Feature ID"));

        let busco_score = registry.lookup("busco_score").unwrap();
        assert_eq!(busco_score.display_group.as_deref(), Some("busco"));
        assert_eq!(busco_score.status.as_deref(), Some("active"));
    }

    #[test]
    fn registry_requires_known_keys_and_reports_unmapped_fields() {
        let registry = AttributeRegistry::load_default().unwrap();

        assert!(registry.require_registered("feature_id").is_ok());
        assert!(registry.require_registered("name").is_ok());
        assert!(registry
            .require_registered("totally_unknown_field")
            .is_err());

        let missing = registry.find_unmapped_keys(vec![
            "feature_id".to_string(),
            "name".to_string(),
            "totally_unknown_field".to_string(),
            "another_missing_field".to_string(),
        ]);

        assert_eq!(
            missing,
            vec!["another_missing_field", "totally_unknown_field"]
        );
    }
}
