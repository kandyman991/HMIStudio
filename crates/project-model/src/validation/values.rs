//! Value-level checks: property values against property types, values
//! against tag data types, member paths and type compatibility.

use serde_json::Value;

use super::checks::{is_hex_color, Validator};
use super::DiagnosticCode as C;
use crate::model::{DataType, LocalizedText, PropertyType};

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Short name of a property type for messages.
pub(super) fn type_name(ty: &PropertyType) -> &'static str {
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

/// Whether a tag of data type `dt` can feed a property of type `ty`
/// directly (without a transform).
pub(super) fn data_type_feeds(dt: &DataType, ty: &PropertyType) -> bool {
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

/// Whether a value parameter of type `param` can feed a property of type
/// `prop`.
pub(super) fn property_type_feeds(param: &PropertyType, prop: &PropertyType) -> bool {
    use PropertyType as P;
    match (param, prop) {
        (P::Bool {}, P::Bool {}) => true,
        (P::Number { .. } | P::Integer { .. }, P::Number { .. }) => true,
        (P::Integer { .. }, P::Integer { .. }) => true,
        (P::String {}, P::String {}) => true,
        (P::String {} | P::Text {}, P::Text {}) => true,
        (P::Color {}, P::Color {}) => true,
        (P::Resource { .. }, P::Resource { .. }) => true,
        (P::Enum { options: a }, P::Enum { options: b }) => a.iter().all(|o| b.contains(o)),
        _ => false,
    }
}

impl Validator<'_> {
    /// Checks operator text (literal or localization key).
    pub fn check_text(&mut self, text: &LocalizedText, path: &str) {
        if let LocalizedText::Key(k) = text {
            if !self.ix.text_keys.contains(k.text_key.as_str()) {
                self.error(
                    C::UnknownReference,
                    path,
                    format!("unknown localization key {:?}", k.text_key),
                );
            }
        }
    }

    /// Checks a property / parameter value against its declared type.
    pub fn check_value(&mut self, value: &Value, ty: &PropertyType, path: &str) {
        let wrong_type = |v: &mut Self| {
            v.error(
                C::InvalidPropertyType,
                path,
                format!("expected {}, found {}", type_name(ty), json_kind(value)),
            )
        };
        match ty {
            PropertyType::Bool {} => {
                if !value.is_boolean() {
                    wrong_type(self);
                }
            }
            PropertyType::Number { min, max } => match value.as_f64() {
                None => wrong_type(self),
                Some(n) => {
                    if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                        self.error(
                            C::InvalidPropertyValue,
                            path,
                            format!("{n} is outside [{min:?}, {max:?}]"),
                        );
                    }
                }
            },
            PropertyType::Integer { min, max } => match value.as_i64() {
                None => {
                    if value.is_u64() {
                        self.error(C::InvalidPropertyValue, path, "integer is too large");
                    } else {
                        wrong_type(self);
                    }
                }
                Some(n) => {
                    if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                        self.error(
                            C::InvalidPropertyValue,
                            path,
                            format!("{n} is outside [{min:?}, {max:?}]"),
                        );
                    }
                }
            },
            PropertyType::String {} => {
                if !value.is_string() {
                    wrong_type(self);
                }
            }
            PropertyType::Text {} => match value {
                Value::String(_) => {}
                Value::Object(o) if o.len() == 1 && o.contains_key("textKey") => {
                    match o.get("textKey").and_then(Value::as_str) {
                        Some(key) => {
                            let key = key.to_owned();
                            self.check_text(&LocalizedText::key(key), path);
                        }
                        None => wrong_type(self),
                    }
                }
                _ => wrong_type(self),
            },
            PropertyType::Color {} => match value {
                Value::String(s) => {
                    if !is_hex_color(s) {
                        self.error(
                            C::InvalidPropertyValue,
                            path,
                            format!("color {s:?} must be #RRGGBB or #RRGGBBAA"),
                        );
                    }
                }
                Value::Object(o) if o.len() == 1 && o.contains_key("token") => {
                    match o.get("token").and_then(Value::as_str) {
                        Some(token) => self.check_color_token(token, path),
                        None => wrong_type(self),
                    }
                }
                _ => wrong_type(self),
            },
            PropertyType::Resource { kinds } => match value.as_str() {
                None => wrong_type(self),
                Some(id) => {
                    let id = crate::model::ResourceId::new_unchecked(id);
                    let kinds = kinds.clone();
                    self.check_resource_ref(&id, &kinds, path);
                }
            },
            PropertyType::Enum { options } => match value.as_str() {
                None => wrong_type(self),
                Some(s) => {
                    if !options.iter().any(|o| o == s) {
                        self.error(
                            C::InvalidPropertyValue,
                            path,
                            format!("{s:?} is not one of {options:?}"),
                        );
                    }
                }
            },
            PropertyType::Tag {
                data_type,
                writable,
            } => match value.as_str() {
                None => wrong_type(self),
                Some(id) => match self.ix.tags.get(id).copied() {
                    None => self.error(C::UnknownReference, path, format!("unknown tag {id:?}")),
                    Some(tag) => {
                        if let Some(dt) = data_type {
                            if tag.data_type != *dt {
                                self.error(
                                    C::InvalidParameter,
                                    path,
                                    format!(
                                        "tag {id:?} is {}, parameter requires {}",
                                        tag.data_type.describe(),
                                        dt.describe()
                                    ),
                                );
                            }
                        }
                        if *writable && !tag.access.writable() {
                            self.error(
                                C::InvalidParameter,
                                path,
                                format!("parameter requires a writable tag; {id:?} is read-only"),
                            );
                        }
                    }
                },
            },
        }
    }

    fn check_color_token(&mut self, token: &str, path: &str) {
        let themes = &self.p.themes;
        let defined = themes
            .iter()
            .filter(|t| t.colors.contains_key(token))
            .count();
        if defined == 0 {
            self.error(
                C::UnknownReference,
                path,
                format!("color token {token:?} is not defined by any theme"),
            );
        } else if defined < themes.len() {
            let missing: Vec<&str> = themes
                .iter()
                .filter(|t| !t.colors.contains_key(token))
                .map(|t| t.id.as_str())
                .collect();
            self.warning(
                C::InvalidStyle,
                path,
                format!("color token {token:?} is missing in themes {missing:?}"),
            );
        }
    }

    /// Checks a JSON value against a tag data type.
    pub fn value_matches_data_type(&self, value: &Value, dt: &DataType) -> Result<(), String> {
        let fail = || {
            Err(format!(
                "value {value} does not match data type {}",
                dt.describe()
            ))
        };
        let int_range = |lo: i128, hi: i128| -> Result<(), String> {
            let n = value
                .as_i64()
                .map(i128::from)
                .or_else(|| value.as_u64().map(i128::from));
            match n {
                Some(n) if n >= lo && n <= hi => Ok(()),
                _ => fail(),
            }
        };
        match dt {
            DataType::Bool => {
                if value.is_boolean() {
                    Ok(())
                } else {
                    fail()
                }
            }
            DataType::Int8 => int_range(i8::MIN.into(), i8::MAX.into()),
            DataType::Int16 => int_range(i16::MIN.into(), i16::MAX.into()),
            DataType::Int32 => int_range(i32::MIN.into(), i32::MAX.into()),
            DataType::Int64 => int_range(i64::MIN.into(), i64::MAX.into()),
            DataType::UInt8 => int_range(0, u8::MAX.into()),
            DataType::UInt16 => int_range(0, u16::MAX.into()),
            DataType::UInt32 => int_range(0, u32::MAX.into()),
            DataType::UInt64 => int_range(0, u64::MAX.into()),
            DataType::Float32 | DataType::Float64 => {
                if value.is_number() {
                    Ok(())
                } else {
                    fail()
                }
            }
            DataType::String | DataType::DateTime => {
                if value.is_string() {
                    Ok(())
                } else {
                    fail()
                }
            }
            DataType::Array { element, length } => match value.as_array() {
                Some(items) if items.len() == *length as usize => {
                    for item in items {
                        self.value_matches_data_type(item, element)?;
                    }
                    Ok(())
                }
                _ => fail(),
            },
            DataType::Struct(id) => {
                let Some(st) = self.ix.data_types.get(id.as_str()) else {
                    return Err(format!("unknown data type {id:?}"));
                };
                let Some(obj) = value.as_object() else {
                    return fail();
                };
                for key in obj.keys() {
                    if !st.members.iter().any(|m| &m.name == key) {
                        return Err(format!("{key:?} is not a member of {id:?}"));
                    }
                }
                for m in &st.members {
                    match obj.get(&m.name) {
                        None => return Err(format!("missing member {:?} of {id:?}", m.name)),
                        Some(v) => self.value_matches_data_type(v, &m.data_type)?,
                    }
                }
                Ok(())
            }
        }
    }

    /// Resolves a member path (`a.b`, `values[3]`, `axes[0].position`)
    /// against a data type.
    pub fn resolve_member(&self, dt: &DataType, member: Option<&str>) -> Result<DataType, String> {
        let Some(path) = member else {
            return Ok(dt.clone());
        };
        if path.is_empty() {
            return Err("member path is empty".into());
        }
        let mut current = dt.clone();
        for (n, segment) in path.split('.').enumerate() {
            let (name, indices) = match segment.find('[') {
                Some(i) => (&segment[..i], &segment[i..]),
                None => (segment, ""),
            };
            if name.is_empty() && !(n == 0 && !indices.is_empty()) {
                return Err(format!("malformed member path {path:?}"));
            }
            if !name.is_empty() {
                let DataType::Struct(id) = &current else {
                    return Err(format!(
                        "member {name:?} accessed on non-struct type {}",
                        current.describe()
                    ));
                };
                let Some(st) = self.ix.data_types.get(id.as_str()) else {
                    return Err(format!("unknown data type {id:?}"));
                };
                let Some(m) = st.members.iter().find(|m| m.name == name) else {
                    return Err(format!("{name:?} is not a member of {id:?}"));
                };
                current = m.data_type.clone();
            }
            let mut rest = indices;
            while !rest.is_empty() {
                let Some(close) = rest.find(']') else {
                    return Err(format!("malformed member path {path:?}"));
                };
                let idx: u32 = rest[1..close]
                    .parse()
                    .map_err(|_| format!("malformed index in member path {path:?}"))?;
                let DataType::Array { element, length } = &current else {
                    return Err(format!("index on non-array type {}", current.describe()));
                };
                if idx >= *length {
                    return Err(format!("index {idx} out of bounds (length {length})"));
                }
                current = (**element).clone();
                rest = &rest[close + 1..];
                if !rest.is_empty() && !rest.starts_with('[') {
                    return Err(format!("malformed member path {path:?}"));
                }
            }
        }
        Ok(current)
    }
}
