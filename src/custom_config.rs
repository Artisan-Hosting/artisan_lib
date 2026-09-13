//! Freeform per-app custom configuration, stored as JSON rather than TOML.
//!
//! `Enviornment_V2` (`crate::enviornment`) covers every field the platform
//! itself needs to know about, but individual apps have always wanted to
//! stash a handful of custom values (see the runtime bundle's design: this
//! is one of the two control-plane config pieces, separate from the `.env`
//! secrets content). TOML plus the `config` crate proved too rigid for that —
//! a missing or oddly-shaped custom field could fail the whole file's parse
//! (the class of bug seen in gitmon). This type is deliberately just a JSON
//! object with typed, fallible-to-`None` accessors instead.

use dusa_collection_utils::core::errors::ErrorArrayItem;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A per-app custom-config document: an arbitrary set of named JSON values.
///
/// Every accessor is fallible-to-`None`/fallible-to-`false` rather than
/// erroring, so one caller's missing or malformed key never breaks another
/// caller reading the same document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomConfig(HashMap<String, Value>);

impl CustomConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads `key` and deserializes it into `T`. Returns `None` if the key is
    /// absent or its value doesn't match `T`'s shape.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.0
            .get(key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    /// Like [`Self::get`], but falls back to `default` instead of `None`.
    pub fn get_or<T: DeserializeOwned>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(default)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn set<T: Serialize>(&mut self, key: &str, value: T) -> Result<(), ErrorArrayItem> {
        let json = serde_json::to_value(value).map_err(ErrorArrayItem::from)?;
        self.0.insert(key.to_owned(), json);
        Ok(())
    }

    pub fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }

    pub fn to_json(&self) -> Result<String, ErrorArrayItem> {
        serde_json::to_string_pretty(&self.0).map_err(ErrorArrayItem::from)
    }

    pub fn from_json(data: &str) -> Result<Self, ErrorArrayItem> {
        let map: HashMap<String, Value> = serde_json::from_str(data).map_err(ErrorArrayItem::from)?;
        Ok(Self(map))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_is_none_not_an_error() {
        let config = CustomConfig::new();
        assert_eq!(config.get::<String>("nope"), None);
        assert_eq!(config.get_or("nope", "fallback".to_string()), "fallback");
    }

    #[test]
    fn wrong_shape_is_none_not_an_error() {
        let mut config = CustomConfig::new();
        config.set("count", "not-a-number").unwrap();
        assert_eq!(config.get::<u32>("count"), None);
    }

    #[test]
    fn round_trips_through_json() {
        let mut config = CustomConfig::new();
        config.set("feature_flag", true).unwrap();
        config.set("retries", 3u32).unwrap();

        let json = config.to_json().unwrap();
        let restored = CustomConfig::from_json(&json).unwrap();

        assert_eq!(restored.get::<bool>("feature_flag"), Some(true));
        assert_eq!(restored.get::<u32>("retries"), Some(3));
        assert!(restored.contains("feature_flag"));
        assert!(!restored.contains("absent"));
    }

    #[test]
    fn remove_drops_a_key() {
        let mut config = CustomConfig::new();
        config.set("temp", 1u32).unwrap();
        config.remove("temp");
        assert!(!config.contains("temp"));
    }
}
