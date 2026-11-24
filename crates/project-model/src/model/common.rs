//! Small shared value types.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Free-form, namespaced engineering metadata.
///
/// Keys should be namespaced (`"vendor.feature"`) to avoid collisions. Values
/// are preserved verbatim through load/save; the model never interprets them.
/// Keys are kept sorted so serialization is deterministic.
pub type Extensions = BTreeMap<String, serde_json::Value>;

/// Text shown to operators: either a literal or a key into
/// [`crate::model::Localization::strings`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LocalizedText {
    /// Literal text (not translated).
    Literal(String),
    /// Reference to a localization key.
    Key(TextKeyRef),
}

/// `{ "textKey": "..." }` reference to a localized string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextKeyRef {
    /// Localization key.
    pub text_key: String,
}

impl LocalizedText {
    /// Literal text.
    pub fn literal(s: impl Into<String>) -> Self {
        Self::Literal(s.into())
    }

    /// Localization key reference.
    pub fn key(s: impl Into<String>) -> Self {
        Self::Key(TextKeyRef { text_key: s.into() })
    }
}

/// Inclusive numeric range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueRange {
    /// Lower bound.
    pub min: f64,
    /// Upper bound.
    pub max: f64,
}

pub(crate) fn is_false(v: &bool) -> bool {
    !*v
}

pub(crate) fn is_true(v: &bool) -> bool {
    *v
}

pub(crate) fn default_true() -> bool {
    true
}

pub(crate) fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

pub(crate) fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

pub(crate) fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

pub(crate) fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

pub(crate) fn default_priority() -> u16 {
    500
}

pub(crate) fn is_default_priority(v: &u16) -> bool {
    *v == 500
}
