//! Stable, typed identifiers.
//!
//! Every addressable object in a project (screens, nodes, tags, alarms, ...)
//! carries an explicit identifier that is stored in the JSON document. The
//! identifier never changes when the object is renamed, moved or restyled,
//! so references, diffs, runtime publishing and the Kotlin/Compose runtime
//! can rely on it.
//!
//! Identifiers are case-sensitive strings matching [`ID_PATTERN`]. They are
//! unique across the **whole** project (all kinds share one namespace), which
//! keeps every reference unambiguous.
//!
//! Deserialization accepts any string so that malformed identifiers are
//! reported by the validator as structured diagnostics (with a path) instead
//! of aborting the load with an opaque parse error.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};

/// Regular expression (ECMA-262 compatible) every identifier must match.
pub const ID_PATTERN: &str = "^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$";

/// Maximum identifier length in bytes.
pub const ID_MAX_LEN: usize = 64;

/// Reason an identifier string is not acceptable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The identifier is empty.
    #[error("identifier is empty")]
    Empty,
    /// The identifier exceeds [`ID_MAX_LEN`].
    #[error("identifier exceeds {ID_MAX_LEN} characters")]
    TooLong,
    /// The identifier does not start with an ASCII letter or digit.
    #[error("identifier must start with an ASCII letter or digit")]
    InvalidStart,
    /// The identifier contains a character outside `[A-Za-z0-9_-]`.
    #[error("identifier contains invalid character {0:?}")]
    InvalidCharacter(char),
}

/// Checks an identifier against [`ID_PATTERN`].
pub fn validate_id(value: &str) -> Result<(), IdError> {
    let mut chars = value.chars();
    let first = chars.next().ok_or(IdError::Empty)?;
    if value.len() > ID_MAX_LEN {
        return Err(IdError::TooLong);
    }
    if !first.is_ascii_alphanumeric() {
        return Err(IdError::InvalidStart);
    }
    for c in chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(IdError::InvalidCharacter(c));
        }
    }
    Ok(())
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates a validated identifier.
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                validate_id(&value)?;
                Ok(Self(value))
            }

            /// Creates an identifier without validation. The validator will
            /// still report it if it is malformed.
            pub fn new_unchecked(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({
                    "description": concat!(
                        "Stable identifier (", stringify!($name),
                        "). Unique across the whole project; never derived from display names."
                    ),
                    "type": "string",
                    "pattern": ID_PATTERN,
                    "minLength": 1,
                    "maxLength": ID_MAX_LEN
                })
            }
        }
    };
}

define_id!(
    /// Identifies a project across files and deployments.
    ProjectId
);
define_id!(
    /// Identifies a screen or popup.
    ScreenId
);
define_id!(
    /// Identifies a node in a layout tree (screen or component definition).
    NodeId
);
define_id!(
    /// Identifies a reusable component definition.
    ComponentDefinitionId
);
define_id!(
    /// Identifies a device / resolution profile.
    DeviceProfileId
);
define_id!(
    /// Identifies a responsive breakpoint.
    BreakpointId
);
define_id!(
    /// Identifies a theme.
    ThemeId
);
define_id!(
    /// Identifies a named style.
    StyleId
);
define_id!(
    /// Identifies a resource / asset.
    ResourceId
);
define_id!(
    /// Identifies a PLC / data-source connection.
    ConnectionId
);
define_id!(
    /// Identifies a tag.
    TagId
);
define_id!(
    /// Identifies a user-defined structured data type.
    DataTypeId
);
define_id!(
    /// Identifies an engineering unit.
    UnitId
);
define_id!(
    /// Identifies an alarm definition.
    AlarmId
);
define_id!(
    /// Identifies a recipe definition.
    RecipeId
);
define_id!(
    /// Identifies a field of a recipe definition.
    RecipeFieldId
);
define_id!(
    /// Identifies a value set of a recipe definition.
    RecipeSetId
);
define_id!(
    /// Identifies a navigation item.
    NavigationItemId
);

define_id!(
    /// Identifies a declared permission (candidate schema v2).
    PermissionId
);
define_id!(
    /// Identifies a declared role (candidate schema v2).
    RoleId
);

define_id!(
    /// Identifies a discrete enumeration (candidate schema v2).
    EnumerationId
);

/// Deterministic allocator for new identifiers.
///
/// Editors use it to create identifiers for new objects. It produces
/// `<prefix>-<n>` with the smallest `n >= 1` not already in use, so the same
/// project state and the same sequence of operations always produce the same
/// identifiers (important for reproducible tests and command replay).
#[derive(Clone, Debug, Default)]
pub struct IdAllocator {
    used: BTreeSet<String>,
}

impl IdAllocator {
    /// Creates an allocator that avoids every identifier in `existing`.
    pub fn new<I, S>(existing: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            used: existing
                .into_iter()
                .map(|s| s.as_ref().to_owned())
                .collect(),
        }
    }

    /// Allocates a new unique identifier with the given prefix.
    ///
    /// The prefix must itself be a valid identifier of at most 40 bytes.
    pub fn allocate(&mut self, prefix: &str) -> Result<String, IdError> {
        validate_id(prefix)?;
        if prefix.len() > 40 {
            return Err(IdError::TooLong);
        }
        let mut n: u64 = 1;
        loop {
            let candidate = format!("{prefix}-{n}");
            if !self.used.contains(&candidate) {
                self.used.insert(candidate.clone());
                return Ok(candidate);
            }
            n += 1;
        }
    }

    /// Marks an identifier as used.
    pub fn reserve(&mut self, id: &str) {
        self.used.insert(id.to_owned());
    }

    /// Whether the identifier is already in use.
    pub fn contains(&self, id: &str) -> bool {
        self.used.contains(id)
    }
}
