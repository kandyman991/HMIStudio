//! Project model types.
//!
//! The model is plain data: no I/O, no UI framework types, no runtime
//! behavior. Every type derives `Serialize`, `Deserialize` and `JsonSchema`
//! and rejects unknown fields so that nothing is silently discarded.

pub mod common;
pub mod component;
pub mod industrial;
pub mod layout;
pub mod project;
pub mod style;
pub mod v2;
pub mod v2_bindings;
pub mod v2_components;

pub use crate::id::*;
pub use common::{Extensions, LocalizedText, TextKeyRef, ValueRange};
pub use component::{
    Action, BindingMode, BindingSource, ComponentDefinition, ComponentKind, EventHandler,
    EventKind, MapEntry, ParameterDefinition, PropertyBinding, PropertyType, TagRef,
    ValueTransform,
};
pub use industrial::{
    AlarmCondition, AlarmDefinition, AlarmSeverity, Connection, DataType, DriverKind,
    EngineeringUnit, RecipeDefinition, RecipeField, RecipeSet, StructMember, StructType, Tag,
    TagAccess,
};
pub use layout::{
    Alignment, Arrangement, Axis, CrossAlignment, Element, GridCell, Insets, LayoutOverride,
    LayoutParams, Node, Position, ResponsiveCondition, ResponsiveRule, ScrollDirection, Size,
    Track, Visibility,
};
pub use project::{
    Breakpoint, DeviceProfile, Localization, Navigation, NavigationItem, Orientation, Platform,
    Project, ProjectMetadata, ProjectSettings, Screen, ScreenKind,
};
pub use style::{Resource, ResourceKind, Style, TextStyle, Theme};
