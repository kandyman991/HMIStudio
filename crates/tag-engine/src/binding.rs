//! Framework-independent binding evaluation.
//!
//! Consumes the RUST-03A contracts unchanged: [`PropertyBinding`],
//! [`BindingSource`], [`BindingMode`], [`ValueTransform`] and the property
//! catalog's [`PropertyType`]s. A binding is first *compiled* against the
//! running tag engine (resolving the tag, member type, access rights and
//! transform compatibility), then evaluated:
//!
//! * **read**: tag snapshot → property value (JSON, as renderers consume it)
//!   with quality and timestamp;
//! * **write**: property value from operator input → inverse transform →
//!   typed tag/member value, written through the engine (access and range
//!   checks apply again there).
//!
//! The compile-time rules mirror the RUST-03A validator so that a runtime
//! never executes a binding the editor would have rejected, even if it is
//! handed an unvalidated project.

use std::collections::BTreeMap;

use hmi_project_model::catalog::{self, Bindability};
use hmi_project_model::model::{
    BindingMode, BindingSource, ComponentDefinition, DataType, Element, Node, Project,
    PropertyBinding, PropertyType, TagId, ValueTransform,
};
use serde::Serialize;
use serde_json::Value as Json;

use crate::engine::{TagEngine, TagError};
use crate::quality::Quality;
use crate::time::Timestamp;
use crate::value::{MemberPath, TagValue, ValueError};

/// Why a binding cannot be compiled or evaluated.
#[derive(Clone, Debug, PartialEq, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BindingError {
    /// The source is a value parameter, not a tag (resolved by renderers).
    #[error("binding of {property:?} uses a value parameter, not a tag")]
    NotATagBinding {
        /// Property.
        property: String,
    },
    /// `tagParameter` not supplied by the instance scope.
    #[error("tag parameter {name:?} is not bound in this scope")]
    UnboundTagParameter {
        /// Parameter name.
        name: String,
    },
    /// Unknown tag.
    #[error("unknown tag {tag:?}")]
    UnknownTag {
        /// Tag.
        tag: String,
    },
    /// Invalid member path.
    #[error("member: {error}")]
    Member {
        /// Cause.
        error: ValueError,
    },
    /// Property cannot be bound / is not an input.
    #[error("property {property:?}: {message}")]
    Property {
        /// Property.
        property: String,
        /// Explanation.
        message: String,
    },
    /// Missing access right.
    #[error("tag {tag:?} is not {operation}able by the HMI")]
    AccessDenied {
        /// Tag.
        tag: String,
        /// `read` or `write`.
        operation: &'static str,
    },
    /// Source type, transform and property type do not fit.
    #[error("incompatible binding: {message}")]
    Incompatible {
        /// Explanation.
        message: String,
    },
    /// Write requested through a transform that cannot be inverted.
    #[error("transform cannot be inverted for writes: {message}")]
    NotInvertible {
        /// Explanation.
        message: String,
    },
    /// Binding is read-only.
    #[error("binding of {property:?} does not write")]
    ReadOnly {
        /// Property.
        property: String,
    },
    /// No `map` entry matched and no default exists.
    #[error("no map entry matches {value}")]
    NoMapping {
        /// Source value.
        value: String,
    },
    /// Operator input has the wrong shape for the property.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Explanation.
        message: String,
    },
    /// Tag engine error.
    #[error(transparent)]
    Tag(#[from] TagError),
}

fn incompatible(message: impl Into<String>) -> BindingError {
    BindingError::Incompatible {
        message: message.into(),
    }
}

fn not_invertible(message: impl Into<String>) -> BindingError {
    BindingError::NotInvertible {
        message: message.into(),
    }
}

fn type_name(ty: &PropertyType) -> &'static str {
    match ty {
        PropertyType::Bool {} => "bool",
        PropertyType::Number { .. } => "number",
        PropertyType::Integer { .. } => "integer",
        PropertyType::String {} => "string",
        PropertyType::Text {} => "text",
        PropertyType::Color {} => "color",
        PropertyType::Resource { .. } => "resource",
        PropertyType::Enum { .. } => "enum",
        PropertyType::Tag { .. } => "tag",
    }
}

/// RUST-03A rule: can a tag of type `dt` drive `ty` without a transform?
fn feeds(dt: &DataType, ty: &PropertyType) -> bool {
    match ty {
        PropertyType::Bool {} => *dt == DataType::Bool,
        PropertyType::Number { .. } => dt.is_numeric(),
        PropertyType::Integer { .. } => dt.is_integer(),
        PropertyType::String {} | PropertyType::Text {} | PropertyType::Enum { .. } => {
            *dt == DataType::String
        }
        PropertyType::Color {} | PropertyType::Resource { .. } | PropertyType::Tag { .. } => false,
    }
}

/// Shape and numeric bounds check of a property value (references such as theme tokens and
/// text keys are validated by RUST-03A at design time, not here).
fn value_fits(v: &Json, ty: &PropertyType) -> bool {
    match ty {
        PropertyType::Bool {} => v.is_boolean(),
        PropertyType::Number { min, max } => {
            if let Some(i) = v
                .as_i64()
                .map(i128::from)
                .or_else(|| v.as_u64().map(i128::from))
            {
                min.is_none_or(|m| m.is_finite() && i >= m.ceil() as i128)
                    && max.is_none_or(|m| m.is_finite() && i <= m.floor() as i128)
            } else {
                v.as_f64().is_some_and(|x| {
                    x.is_finite() && min.is_none_or(|m| x >= m) && max.is_none_or(|m| x <= m)
                })
            }
        }
        PropertyType::Integer { min, max } => v
            .as_i64()
            .map(i128::from)
            .or_else(|| v.as_u64().map(i128::from))
            .is_some_and(|i| {
                min.is_none_or(|m| i >= i128::from(m)) && max.is_none_or(|m| i <= i128::from(m))
            }),
        PropertyType::String {} => v.is_string(),
        PropertyType::Enum { options } => {
            v.as_str().is_some_and(|s| options.iter().any(|o| o == s))
        }
        PropertyType::Text {} => {
            v.is_string()
                || v.as_object()
                    .is_some_and(|o| o.len() == 1 && o.contains_key("textKey"))
        }
        PropertyType::Color {} => {
            v.as_str().is_some_and(|s| {
                s.strip_prefix('#').is_some_and(|h| {
                    (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit())
                })
            }) || v
                .as_object()
                .is_some_and(|o| o.len() == 1 && o.contains_key("token"))
        }
        PropertyType::Resource { .. } => v.is_string(),
        PropertyType::Tag { .. } => v.is_string(),
    }
}

/// Instance scope: values supplied to a component definition's `tag`
/// parameters (from `Element::Instance::parameters`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingScope {
    /// Tag parameter name -> tag.
    pub tag_parameters: BTreeMap<String, TagId>,
}

impl BindingScope {
    /// Scope of an instance node of `definition`: every `tag` parameter
    /// whose supplied value is a tag id string.
    pub fn for_instance(definition: &ComponentDefinition, instance: &Node) -> Self {
        let mut tag_parameters = BTreeMap::new();
        if let Element::Instance { parameters, .. } = &instance.element {
            for p in &definition.parameters {
                if matches!(p.ty, PropertyType::Tag { .. }) {
                    if let Some(id) = parameters.get(&p.name).and_then(Json::as_str) {
                        tag_parameters.insert(p.name.clone(), TagId::new_unchecked(id));
                    }
                }
            }
        }
        Self { tag_parameters }
    }
}

/// A binding resolved against the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledBinding {
    /// Target property.
    pub property: String,
    /// Property type.
    pub property_type: PropertyType,
    /// Mode.
    pub mode: BindingMode,
    /// Source tag.
    pub tag: TagId,
    /// Member within the tag.
    pub member: MemberPath,
    /// Type at `member`.
    pub source_type: DataType,
    /// Transform.
    pub transform: Option<ValueTransform>,
}

/// Result of a read evaluation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundValue {
    /// Property value for the renderer.
    pub value: Json,
    /// Source quality (renderers show bad quality distinctly).
    pub quality: Quality,
    /// Source timestamp.
    pub timestamp: Timestamp,
}

/// Property type and bindability of `property` on `node` (component
/// catalog, or a definition's value parameter for instances).
pub fn property_target(
    project: &Project,
    node: &Node,
    property: &str,
) -> Option<(PropertyType, Bindability)> {
    match &node.element {
        Element::Instance { definition, .. } => {
            if let Some(spec) = catalog::find_property(None, property) {
                return Some((spec.ty, spec.bindable));
            }
            let def = project.components.iter().find(|d| &d.id == definition)?;
            let p = def.parameters.iter().find(|p| p.name == property)?;
            (!matches!(p.ty, PropertyType::Tag { .. })).then(|| (p.ty.clone(), Bindability::Read))
        }
        Element::Component { kind } => {
            catalog::find_property(Some(*kind), property).map(|s| (s.ty, s.bindable))
        }
        _ => catalog::find_property(None, property).map(|s| (s.ty, s.bindable)),
    }
}

/// Compiles a binding of `node` (property type from the catalog).
pub fn compile_for_node(
    project: &Project,
    node: &Node,
    binding: &PropertyBinding,
    scope: &BindingScope,
    engine: &TagEngine,
) -> Result<CompiledBinding, BindingError> {
    let (ty, bindable) = property_target(project, node, &binding.property).ok_or_else(|| {
        BindingError::Property {
            property: binding.property.clone(),
            message: "unknown property".into(),
        }
    })?;
    if bindable == Bindability::Static {
        return Err(BindingError::Property {
            property: binding.property.clone(),
            message: "property cannot be bound".into(),
        });
    }
    if binding.mode.writes() && bindable != Bindability::ReadWrite {
        return Err(BindingError::Property {
            property: binding.property.clone(),
            message: "property is not an operator input".into(),
        });
    }
    compile(binding, &ty, scope, engine)
}

/// Compiles a binding for a property of type `property_type`.
pub fn compile(
    binding: &PropertyBinding,
    property_type: &PropertyType,
    scope: &BindingScope,
    engine: &TagEngine,
) -> Result<CompiledBinding, BindingError> {
    let tag = match &binding.source {
        BindingSource::Tag(t) => t.clone(),
        BindingSource::TagParameter(name) => scope
            .tag_parameters
            .get(name)
            .cloned()
            .ok_or_else(|| BindingError::UnboundTagParameter { name: name.clone() })?,
        BindingSource::Parameter(_) => {
            return Err(BindingError::NotATagBinding {
                property: binding.property.clone(),
            })
        }
    };
    let def = engine
        .definition(&tag)
        .ok_or_else(|| BindingError::UnknownTag {
            tag: tag.to_string(),
        })?;
    let member = MemberPath::parse_opt(binding.member.as_deref())
        .map_err(|error| BindingError::Member { error })?;
    let source_type = engine
        .types()
        .member_type(&def.data_type, &member)
        .map_err(|error| BindingError::Member { error })?;
    if binding.mode.reads() && !def.access.readable() {
        return Err(BindingError::AccessDenied {
            tag: tag.to_string(),
            operation: "read",
        });
    }
    if binding.mode.writes() && !def.access.writable() {
        return Err(BindingError::AccessDenied {
            tag: tag.to_string(),
            operation: "write",
        });
    }
    check_transform(
        binding.transform.as_ref(),
        &source_type,
        property_type,
        binding.mode,
        engine,
    )?;
    Ok(CompiledBinding {
        property: binding.property.clone(),
        property_type: property_type.clone(),
        mode: binding.mode,
        tag,
        member,
        source_type,
        transform: binding.transform.clone(),
    })
}

fn check_transform(
    t: Option<&ValueTransform>,
    dt: &DataType,
    ty: &PropertyType,
    mode: BindingMode,
    engine: &TagEngine,
) -> Result<(), BindingError> {
    let numeric_prop = matches!(
        ty,
        PropertyType::Number { .. } | PropertyType::Integer { .. }
    );
    match t {
        None => {
            if !feeds(dt, ty) {
                return Err(incompatible(format!(
                    "a {} value cannot drive a {} property without a transform",
                    dt.describe(),
                    type_name(ty)
                )));
            }
        }
        Some(ValueTransform::Linear { factor, offset }) => {
            if !factor.is_finite() || !offset.is_finite() {
                return Err(incompatible("linear factor and offset must be finite"));
            }
            if !dt.is_numeric() || !numeric_prop {
                return Err(incompatible("linear needs a numeric source and property"));
            }
            if mode.writes() && *factor == 0.0 {
                return Err(not_invertible("linear factor 0"));
            }
        }
        Some(ValueTransform::Invert {}) => {
            if *dt != DataType::Bool || *ty != (PropertyType::Bool {}) {
                return Err(incompatible("invert needs a bool source and property"));
            }
        }
        Some(ValueTransform::Bit { index }) => {
            match dt.integer_bits() {
                Some(bits) if *index < bits => {}
                Some(bits) => {
                    return Err(incompatible(format!(
                        "bit {index} out of range for a {bits}-bit value"
                    )))
                }
                None => return Err(incompatible("bit needs an integer source")),
            }
            if *ty != (PropertyType::Bool {}) {
                return Err(incompatible("bit needs a bool property"));
            }
            if mode.writes() {
                return Err(not_invertible("bit extraction"));
            }
        }
        Some(ValueTransform::Map { entries, default }) => {
            if mode.writes() {
                return Err(not_invertible("map"));
            }
            if entries.is_empty() {
                return Err(incompatible("map has no entries"));
            }
            for e in entries {
                engine
                    .types()
                    .value_from_json(&e.when, dt)
                    .map_err(|err| incompatible(format!("map key {}: {err}", e.when)))?;
                if !value_fits(&e.value, ty) {
                    return Err(incompatible(format!(
                        "map value {} is not a valid {}",
                        e.value,
                        type_name(ty)
                    )));
                }
            }
            if let Some(d) = default {
                if !value_fits(d, ty) {
                    return Err(incompatible(format!(
                        "map default {d} is not a valid {}",
                        type_name(ty)
                    )));
                }
            }
        }
    }
    Ok(())
}

fn number(x: f64) -> Result<Json, BindingError> {
    serde_json::Number::from_f64(x)
        .map(Json::Number)
        .ok_or_else(|| incompatible("non-finite result"))
}

fn numeric_json(x: f64, ty: &PropertyType) -> Result<Json, BindingError> {
    match ty {
        // Integer properties receive the nearest integer.
        PropertyType::Integer { .. } => {
            let r = x.round();
            if r.abs() < 9.0e15 {
                Ok(Json::from(r as i64))
            } else {
                Err(incompatible(format!(
                    "{x} does not fit an integer property"
                )))
            }
        }
        _ => number(x),
    }
}

/// Exact typed equality of a source value with a map key.
fn map_matches(v: &TagValue, key: &TagValue) -> bool {
    // Map keys are parsed to the exact source type, so typed equality
    // distinguishes adjacent wide integers instead of comparing rounded f64s.
    v == key
}

impl CompiledBinding {
    /// Source → property. Applies to `read` and `readWrite` bindings.
    pub fn evaluate_value(
        &self,
        source: &TagValue,
        engine: &TagEngine,
    ) -> Result<Json, BindingError> {
        let v = source;
        match &self.transform {
            None => match &self.property_type {
                PropertyType::Bool {} => v
                    .as_bool()
                    .map(Json::Bool)
                    .ok_or_else(|| incompatible("expected bool")),
                PropertyType::Number { .. } | PropertyType::Integer { .. } => {
                    // Integer sources stay integers in JSON (u64 included).
                    if let Some(i) = v.as_i128() {
                        return i64::try_from(i)
                            .map(Json::from)
                            .or_else(|_| u64::try_from(i).map(Json::from))
                            .map_err(|_| incompatible("integer too large"));
                    }
                    let x = v.as_f64().ok_or_else(|| incompatible("expected number"))?;
                    numeric_json(x, &self.property_type)
                }
                _ => match v {
                    TagValue::String(s) => Ok(Json::String(s.clone())),
                    _ => Err(incompatible("expected string")),
                },
            },
            Some(ValueTransform::Linear { factor, offset }) => {
                if v.as_i128().is_some_and(|i| i.abs() > 9_007_199_254_740_991) {
                    return Err(incompatible(
                        "wide integer cannot safely enter a linear transform",
                    ));
                }
                let x = v.as_f64().ok_or_else(|| incompatible("expected number"))?;
                numeric_json(x * factor + offset, &self.property_type)
            }
            Some(ValueTransform::Invert {}) => v
                .as_bool()
                .map(|b| Json::Bool(!b))
                .ok_or_else(|| incompatible("expected bool")),
            Some(ValueTransform::Bit { index }) => {
                let bits = self.source_type.integer_bits().unwrap_or(64);
                let i = v
                    .as_i128()
                    .ok_or_else(|| incompatible("expected integer"))?;
                // Two's complement within the source width.
                let mask: u128 = if bits >= 128 {
                    u128::MAX
                } else {
                    (1u128 << bits) - 1
                };
                let raw = (i as u128) & mask;
                Ok(Json::Bool((raw >> index) & 1 == 1))
            }
            Some(ValueTransform::Map { entries, default }) => {
                for e in entries {
                    if let Ok(key) = engine.types().value_from_json(&e.when, &self.source_type) {
                        if map_matches(v, &key) {
                            return Ok(e.value.clone());
                        }
                    }
                }
                default.clone().ok_or_else(|| BindingError::NoMapping {
                    value: v.to_string(),
                })
            }
        }
    }

    /// Reads the current tag value and evaluates it.
    pub fn read(&self, engine: &TagEngine) -> Result<BoundValue, BindingError> {
        if !self.mode.reads() {
            return Err(BindingError::Property {
                property: self.property.clone(),
                message: "write-only binding".into(),
            });
        }
        let snap = engine.read(&self.tag)?;
        let v = snap
            .value
            .get(&self.member)
            .ok_or_else(|| BindingError::Member {
                error: ValueError::MalformedPath(self.member.to_string()),
            })?;
        Ok(BoundValue {
            value: self.evaluate_value(v, engine)?,
            quality: snap.quality,
            timestamp: snap.timestamp,
        })
    }

    /// Property → typed source value (inverse transform). Integer targets
    /// receive the nearest integer after a linear inverse; out-of-type
    /// values are rejected.
    pub fn inverse_value(
        &self,
        input: &Json,
        engine: &TagEngine,
    ) -> Result<TagValue, BindingError> {
        if !self.mode.writes() {
            return Err(BindingError::ReadOnly {
                property: self.property.clone(),
            });
        }
        if !value_fits(input, &self.property_type) {
            return Err(BindingError::InvalidInput {
                message: format!("{input} is not a valid {}", type_name(&self.property_type)),
            });
        }
        let raw = match &self.transform {
            None => match input {
                Json::Bool(b) => TagValue::Bool(*b),
                Json::String(s) => TagValue::String(s.clone()),
                Json::Number(n) => match (n.as_i64(), n.as_u64(), n.as_f64()) {
                    (Some(i), _, _) => TagValue::Int64(i),
                    (_, Some(u), _) => TagValue::UInt64(u),
                    (_, _, Some(f)) => TagValue::Float64(f),
                    _ => return Err(incompatible("unrepresentable number")),
                },
                _ => return Err(incompatible("unsupported input")),
            },
            Some(ValueTransform::Linear { factor, offset }) => {
                if *factor == 0.0 {
                    return Err(not_invertible("linear factor 0"));
                }
                if input
                    .as_i64()
                    .map(i128::from)
                    .or_else(|| input.as_u64().map(i128::from))
                    .is_some_and(|i| i.abs() > 9_007_199_254_740_991)
                {
                    return Err(incompatible(
                        "wide integer cannot safely enter a linear inverse",
                    ));
                }
                let x = input
                    .as_f64()
                    .ok_or_else(|| incompatible("expected number"))?;
                let mut y = (x - offset) / factor;
                if self.source_type.is_integer() {
                    y = y.round();
                    if y.abs() > 9_007_199_254_740_991.0 {
                        return Err(incompatible(
                            "linear inverse exceeds safe integer precision",
                        ));
                    }
                }
                TagValue::Float64(y)
            }
            Some(ValueTransform::Invert {}) => TagValue::Bool(
                !input
                    .as_bool()
                    .ok_or_else(|| incompatible("expected bool"))?,
            ),
            Some(ValueTransform::Bit { .. }) => return Err(not_invertible("bit extraction")),
            Some(ValueTransform::Map { .. }) => return Err(not_invertible("map")),
        };
        engine
            .types()
            .coerce(raw, &self.source_type)
            .map_err(|error| BindingError::Member { error })
    }

    /// Writes operator input through the binding.
    pub fn write(&self, input: &Json, engine: &TagEngine) -> Result<(), BindingError> {
        let v = self.inverse_value(input, engine)?;
        engine.write_member(&self.tag, &self.member, v)?;
        Ok(())
    }
}
