//! Typed runtime tag values.
//!
//! [`TagValue`] is the in-memory representation of a value of a RUST-03A
//! [`DataType`]. The authoritative *types* stay in the project model; this
//! module only adds runtime values, conversion from/to the JSON value form
//! used by the project document (`initialValue`, recipe values, action
//! values), member addressing and strict type checking.

use std::collections::BTreeMap;
use std::fmt;

use hmi_project_model::model::{DataType, DataTypeId, Project, StructType};
use serde_json::Value as Json;

/// Maximum struct/array nesting depth processed (guards cyclic or absurd
/// type definitions; RUST-03A rejects cyclic structs at validation).
pub const MAX_TYPE_DEPTH: usize = 32;
/// Maximum number of expanded value nodes in a default value.
pub const MAX_DEFAULT_VALUE_NODES: usize = 100_000;

/// A runtime value.
#[derive(Clone, Debug, PartialEq)]
pub enum TagValue {
    /// `bool`.
    Bool(bool),
    /// `int8`.
    Int8(i8),
    /// `int16`.
    Int16(i16),
    /// `int32`.
    Int32(i32),
    /// `int64`.
    Int64(i64),
    /// `uint8`.
    UInt8(u8),
    /// `uint16`.
    UInt16(u16),
    /// `uint32`.
    UInt32(u32),
    /// `uint64`.
    UInt64(u64),
    /// `float32`.
    Float32(f32),
    /// `float64`.
    Float64(f64),
    /// `string`.
    String(String),
    /// `dateTime` (ISO-8601 text, not parsed in this milestone).
    DateTime(String),
    /// Fixed-length array.
    Array(Vec<TagValue>),
    /// Structure: member name -> value.
    Struct(BTreeMap<String, TagValue>),
}

impl TagValue {
    /// Short kind name for messages.
    pub fn kind(&self) -> &'static str {
        match self {
            TagValue::Bool(_) => "bool",
            TagValue::Int8(_) => "int8",
            TagValue::Int16(_) => "int16",
            TagValue::Int32(_) => "int32",
            TagValue::Int64(_) => "int64",
            TagValue::UInt8(_) => "uint8",
            TagValue::UInt16(_) => "uint16",
            TagValue::UInt32(_) => "uint32",
            TagValue::UInt64(_) => "uint64",
            TagValue::Float32(_) => "float32",
            TagValue::Float64(_) => "float64",
            TagValue::String(_) => "string",
            TagValue::DateTime(_) => "dateTime",
            TagValue::Array(_) => "array",
            TagValue::Struct(_) => "struct",
        }
    }

    /// Integer value (any integer kind) widened to i128.
    pub fn as_i128(&self) -> Option<i128> {
        Some(match self {
            TagValue::Int8(v) => i128::from(*v),
            TagValue::Int16(v) => i128::from(*v),
            TagValue::Int32(v) => i128::from(*v),
            TagValue::Int64(v) => i128::from(*v),
            TagValue::UInt8(v) => i128::from(*v),
            TagValue::UInt16(v) => i128::from(*v),
            TagValue::UInt32(v) => i128::from(*v),
            TagValue::UInt64(v) => i128::from(*v),
            _ => return None,
        })
    }

    /// Numeric value as f64 (integers may lose precision above 2^53).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            TagValue::Float32(v) => Some(f32_to_f64_display(*v)),
            TagValue::Float64(v) => Some(*v),
            other => other.as_i128().map(|i| i as f64),
        }
    }

    /// Boolean value.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TagValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// JSON form (as used in project documents and UI snapshots).
    pub fn to_json(&self) -> Json {
        match self {
            TagValue::Bool(v) => Json::Bool(*v),
            TagValue::Int8(v) => Json::from(*v),
            TagValue::Int16(v) => Json::from(*v),
            TagValue::Int32(v) => Json::from(*v),
            TagValue::Int64(v) => Json::from(*v),
            TagValue::UInt8(v) => Json::from(*v),
            TagValue::UInt16(v) => Json::from(*v),
            TagValue::UInt32(v) => Json::from(*v),
            TagValue::UInt64(v) => Json::from(*v),
            TagValue::Float32(v) => number(f32_to_f64_display(*v)),
            TagValue::Float64(v) => number(*v),
            TagValue::String(s) | TagValue::DateTime(s) => Json::String(s.clone()),
            TagValue::Array(items) => Json::Array(items.iter().map(TagValue::to_json).collect()),
            TagValue::Struct(m) => {
                Json::Object(m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
            }
        }
    }

    /// Member at `path` (empty path = self).
    pub fn get(&self, path: &MemberPath) -> Option<&TagValue> {
        let mut cur = self;
        for seg in &path.0 {
            cur = match (seg, cur) {
                (Segment::Field(f), TagValue::Struct(m)) => m.get(f)?,
                (Segment::Index(i), TagValue::Array(a)) => a.get(*i as usize)?,
                _ => return None,
            };
        }
        Some(cur)
    }

    /// Mutable member at `path`.
    pub fn get_mut(&mut self, path: &MemberPath) -> Option<&mut TagValue> {
        let mut cur = self;
        for seg in &path.0 {
            cur = match (seg, cur) {
                (Segment::Field(f), TagValue::Struct(m)) => m.get_mut(f)?,
                (Segment::Index(i), TagValue::Array(a)) => a.get_mut(*i as usize)?,
                _ => return None,
            };
        }
        Some(cur)
    }
}

/// f32 → f64 preserving the shortest decimal representation (0.1f32 → 0.1
/// rather than 0.10000000149011612), so values look as entered.
pub fn f32_to_f64_display(v: f32) -> f64 {
    if !v.is_finite() {
        return f64::from(v);
    }
    format!("{v}").parse().unwrap_or(f64::from(v))
}

fn number(v: f64) -> Json {
    serde_json::Number::from_f64(v).map_or(Json::Null, Json::Number)
}

impl fmt::Display for TagValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_json())
    }
}

/// One step of a member path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Segment {
    /// Struct member.
    Field(String),
    /// Array element.
    Index(u32),
}

/// Parsed member path, using the RUST-03A syntax: `a.b`, `values[3]`,
/// `axes[0].position`, `[2]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemberPath(pub Vec<Segment>);

impl MemberPath {
    /// The empty path (whole value).
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// Parses a path; `None` / empty input is rejected by the caller.
    pub fn parse(path: &str) -> Result<Self, ValueError> {
        let bad = || ValueError::MalformedPath(path.to_owned());
        if path.is_empty() {
            return Err(bad());
        }
        let mut segs = Vec::new();
        for (n, part) in path.split('.').enumerate() {
            let (name, mut rest) = match part.find('[') {
                Some(i) => (&part[..i], &part[i..]),
                None => (part, ""),
            };
            if name.is_empty() {
                if n != 0 || rest.is_empty() {
                    return Err(bad());
                }
            } else {
                let mut chars = name.chars();
                let ok_start = chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
                if !ok_start || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    return Err(bad());
                }
                segs.push(Segment::Field(name.to_owned()));
            }
            while !rest.is_empty() {
                let close = rest.find(']').ok_or_else(bad)?;
                if !rest.starts_with('[') {
                    return Err(bad());
                }
                let idx: u32 = rest[1..close].parse().map_err(|_| bad())?;
                segs.push(Segment::Index(idx));
                rest = &rest[close + 1..];
            }
        }
        Ok(Self(segs))
    }

    /// Parses an optional member string (`None` = whole value).
    pub fn parse_opt(path: Option<&str>) -> Result<Self, ValueError> {
        path.map_or_else(|| Ok(Self::root()), Self::parse)
    }

    /// Whether this is the whole value.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for MemberPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, s) in self.0.iter().enumerate() {
            match s {
                Segment::Field(n) if i == 0 => write!(f, "{n}")?,
                Segment::Field(n) => write!(f, ".{n}")?,
                Segment::Index(x) => write!(f, "[{x}]")?,
            }
        }
        Ok(())
    }
}

/// Value / type error.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ValueError {
    /// Wrong value type.
    #[error("expected {expected}, found {found}")]
    TypeMismatch {
        /// Expected type.
        expected: String,
        /// Actual value.
        found: String,
    },
    /// Number outside the data type's representable range.
    #[error("{value} is out of range for {data_type}")]
    OutOfRange {
        /// Offending value.
        value: String,
        /// Target type.
        data_type: String,
    },
    /// Non-finite floating-point value.
    #[error("non-finite number")]
    NonFinite,
    /// Unknown struct data type.
    #[error("unknown data type {0:?}")]
    UnknownDataType(String),
    /// Member does not exist.
    #[error("{member:?} is not a member of {data_type}")]
    UnknownMember {
        /// Member name.
        member: String,
        /// Type.
        data_type: String,
    },
    /// Array index too large.
    #[error("index {index} out of bounds (length {length})")]
    IndexOutOfBounds {
        /// Index.
        index: u32,
        /// Length.
        length: u32,
    },
    /// Malformed member path.
    #[error("malformed member path {0:?}")]
    MalformedPath(String),
    /// Type definitions nested too deeply (or cyclic).
    #[error("type nesting exceeds {MAX_TYPE_DEPTH} levels")]
    TooDeep,
    /// Expanded default value would exceed the runtime allocation limit.
    #[error("default value exceeds {MAX_DEFAULT_VALUE_NODES} nodes")]
    TooLarge,
}

fn mismatch(dt: &DataType, found: impl Into<String>) -> ValueError {
    ValueError::TypeMismatch {
        expected: dt.describe(),
        found: found.into(),
    }
}

fn json_kind(v: &Json) -> String {
    match v {
        Json::Null => "null".into(),
        Json::Bool(_) => "bool".into(),
        Json::Number(n) => format!("number {n}"),
        Json::String(_) => "string".into(),
        Json::Array(_) => "array".into(),
        Json::Object(_) => "object".into(),
    }
}

/// Struct definitions of a project; converts and checks values.
#[derive(Clone, Debug, Default)]
pub struct TypeContext {
    structs: BTreeMap<String, StructType>,
}

macro_rules! int_from {
    ($variant:ident, $t:ty, $n:expr, $dt:expr) => {{
        let n: i128 = $n;
        <$t>::try_from(n)
            .map(TagValue::$variant)
            .map_err(|_| ValueError::OutOfRange {
                value: n.to_string(),
                data_type: $dt.describe(),
            })
    }};
}

impl TypeContext {
    /// Collects the project's struct types (first definition wins on
    /// duplicate ids, matching the validator's index).
    pub fn from_project(p: &Project) -> Self {
        let mut structs = BTreeMap::new();
        for t in &p.data_types {
            structs.entry(t.id.to_string()).or_insert_with(|| t.clone());
        }
        Self { structs }
    }

    fn struct_type(&self, id: &DataTypeId) -> Result<&StructType, ValueError> {
        self.structs
            .get(id.as_str())
            .ok_or_else(|| ValueError::UnknownDataType(id.to_string()))
    }

    /// Zero / empty / false value of a type.
    pub fn default_value(&self, dt: &DataType) -> Result<TagValue, ValueError> {
        self.default_size(dt, 0)?;
        self.default_at(dt, 0)
    }

    fn default_size(&self, dt: &DataType, depth: usize) -> Result<usize, ValueError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ValueError::TooDeep);
        }
        let add = |a: usize, b: usize| {
            a.checked_add(b)
                .filter(|n| *n <= MAX_DEFAULT_VALUE_NODES)
                .ok_or(ValueError::TooLarge)
        };
        match dt {
            DataType::Array { element, length } => {
                let size = self.default_size(element, depth + 1)?;
                let size = size
                    .checked_mul(*length as usize)
                    .ok_or(ValueError::TooLarge)?;
                add(1, size)
            }
            DataType::Struct(id) => {
                let mut size = 1;
                for m in &self.struct_type(id)?.members {
                    size = add(size, self.default_size(&m.data_type, depth + 1)?)?;
                }
                Ok(size)
            }
            _ => Ok(1),
        }
    }

    fn default_at(&self, dt: &DataType, depth: usize) -> Result<TagValue, ValueError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ValueError::TooDeep);
        }
        Ok(match dt {
            DataType::Bool => TagValue::Bool(false),
            DataType::Int8 => TagValue::Int8(0),
            DataType::Int16 => TagValue::Int16(0),
            DataType::Int32 => TagValue::Int32(0),
            DataType::Int64 => TagValue::Int64(0),
            DataType::UInt8 => TagValue::UInt8(0),
            DataType::UInt16 => TagValue::UInt16(0),
            DataType::UInt32 => TagValue::UInt32(0),
            DataType::UInt64 => TagValue::UInt64(0),
            DataType::Float32 => TagValue::Float32(0.0),
            DataType::Float64 => TagValue::Float64(0.0),
            DataType::String => TagValue::String(String::new()),
            DataType::DateTime => TagValue::DateTime(String::new()),
            DataType::Array { element, length } => {
                let e = self.default_at(element, depth + 1)?;
                TagValue::Array(vec![e; *length as usize])
            }
            DataType::Struct(id) => {
                let st = self.struct_type(id)?;
                let mut m = BTreeMap::new();
                for member in &st.members {
                    m.insert(
                        member.name.clone(),
                        self.default_at(&member.data_type, depth + 1)?,
                    );
                }
                TagValue::Struct(m)
            }
        })
    }

    /// Strictly converts a JSON value to a typed value (same rules as the
    /// RUST-03A validator: exact integers in range, finite floats within the
    /// type's range, exact array length, exactly the struct's members).
    pub fn value_from_json(&self, v: &Json, dt: &DataType) -> Result<TagValue, ValueError> {
        self.value_from_json_at(v, dt, 0)
    }

    fn value_from_json_at(
        &self,
        v: &Json,
        dt: &DataType,
        depth: usize,
    ) -> Result<TagValue, ValueError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ValueError::TooDeep);
        }
        let int = || -> Result<i128, ValueError> {
            v.as_i64()
                .map(i128::from)
                .or_else(|| v.as_u64().map(i128::from))
                .ok_or_else(|| mismatch(dt, json_kind(v)))
        };
        match dt {
            DataType::Bool => v
                .as_bool()
                .map(TagValue::Bool)
                .ok_or_else(|| mismatch(dt, json_kind(v))),
            DataType::Int8 => int_from!(Int8, i8, int()?, dt),
            DataType::Int16 => int_from!(Int16, i16, int()?, dt),
            DataType::Int32 => int_from!(Int32, i32, int()?, dt),
            DataType::Int64 => int_from!(Int64, i64, int()?, dt),
            DataType::UInt8 => int_from!(UInt8, u8, int()?, dt),
            DataType::UInt16 => int_from!(UInt16, u16, int()?, dt),
            DataType::UInt32 => int_from!(UInt32, u32, int()?, dt),
            DataType::UInt64 => int_from!(UInt64, u64, int()?, dt),
            DataType::Float32 => {
                let f = v.as_f64().ok_or_else(|| mismatch(dt, json_kind(v)))?;
                float32(f, dt)
            }
            DataType::Float64 => {
                let f = v.as_f64().ok_or_else(|| mismatch(dt, json_kind(v)))?;
                if f.is_finite() {
                    Ok(TagValue::Float64(f))
                } else {
                    Err(ValueError::NonFinite)
                }
            }
            DataType::String => v
                .as_str()
                .map(|s| TagValue::String(s.to_owned()))
                .ok_or_else(|| mismatch(dt, json_kind(v))),
            DataType::DateTime => v
                .as_str()
                .map(|s| TagValue::DateTime(s.to_owned()))
                .ok_or_else(|| mismatch(dt, json_kind(v))),
            DataType::Array { element, length } => {
                let items = v.as_array().ok_or_else(|| mismatch(dt, json_kind(v)))?;
                if items.len() != *length as usize {
                    return Err(mismatch(dt, format!("array of length {}", items.len())));
                }
                items
                    .iter()
                    .map(|x| self.value_from_json_at(x, element, depth + 1))
                    .collect::<Result<_, _>>()
                    .map(TagValue::Array)
            }
            DataType::Struct(id) => {
                let st = self.struct_type(id)?;
                let obj = v.as_object().ok_or_else(|| mismatch(dt, json_kind(v)))?;
                if let Some(k) = obj
                    .keys()
                    .find(|k| !st.members.iter().any(|m| &m.name == *k))
                {
                    return Err(ValueError::UnknownMember {
                        member: k.clone(),
                        data_type: dt.describe(),
                    });
                }
                let mut m = BTreeMap::new();
                for member in &st.members {
                    let x = obj.get(&member.name).ok_or_else(|| {
                        mismatch(dt, format!("object without member {:?}", member.name))
                    })?;
                    m.insert(
                        member.name.clone(),
                        self.value_from_json_at(x, &member.data_type, depth + 1)?,
                    );
                }
                Ok(TagValue::Struct(m))
            }
        }
    }

    /// Checks that `v` is exactly of type `dt`.
    pub fn check(&self, v: &TagValue, dt: &DataType) -> Result<(), ValueError> {
        self.check_at(v, dt, 0)
    }

    fn check_at(&self, v: &TagValue, dt: &DataType, depth: usize) -> Result<(), ValueError> {
        if depth > MAX_TYPE_DEPTH {
            return Err(ValueError::TooDeep);
        }
        let ok = match (dt, v) {
            (DataType::Bool, TagValue::Bool(_))
            | (DataType::Int8, TagValue::Int8(_))
            | (DataType::Int16, TagValue::Int16(_))
            | (DataType::Int32, TagValue::Int32(_))
            | (DataType::Int64, TagValue::Int64(_))
            | (DataType::UInt8, TagValue::UInt8(_))
            | (DataType::UInt16, TagValue::UInt16(_))
            | (DataType::UInt32, TagValue::UInt32(_))
            | (DataType::UInt64, TagValue::UInt64(_))
            | (DataType::String, TagValue::String(_))
            | (DataType::DateTime, TagValue::DateTime(_)) => true,
            (DataType::Float32, TagValue::Float32(f)) => {
                if !f.is_finite() {
                    return Err(ValueError::NonFinite);
                }
                true
            }
            (DataType::Float64, TagValue::Float64(f)) => {
                if !f.is_finite() {
                    return Err(ValueError::NonFinite);
                }
                true
            }
            (DataType::Array { element, length }, TagValue::Array(items)) => {
                if items.len() != *length as usize {
                    return Err(mismatch(dt, format!("array of length {}", items.len())));
                }
                for x in items {
                    self.check_at(x, element, depth + 1)?;
                }
                true
            }
            (DataType::Struct(id), TagValue::Struct(m)) => {
                let st = self.struct_type(id)?;
                if m.len() != st.members.len() {
                    return Err(mismatch(dt, "struct with different members"));
                }
                for member in &st.members {
                    let x = m
                        .get(&member.name)
                        .ok_or_else(|| ValueError::UnknownMember {
                            member: member.name.clone(),
                            data_type: dt.describe(),
                        })?;
                    self.check_at(x, &member.data_type, depth + 1)?;
                }
                true
            }
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(mismatch(dt, v.kind()))
        }
    }

    /// Converts a value to type `dt` where this is lossless in kind:
    /// numeric → integer when integral and in range; numeric → float when
    /// finite and in range (integer → float may round above 2^24 / 2^53);
    /// everything else must already match exactly. Never converts between
    /// bool, numbers and strings.
    pub fn coerce(&self, v: TagValue, dt: &DataType) -> Result<TagValue, ValueError> {
        if self.check(&v, dt).is_ok() {
            return Ok(v);
        }
        let numeric_in = v.as_f64();
        let int_in = v.as_i128();
        let as_int = |f: f64| -> Result<i128, ValueError> {
            if f.fract() == 0.0 && f.is_finite() && f.abs() < 1.8e19 {
                Ok(f as i128)
            } else {
                Err(ValueError::OutOfRange {
                    value: f.to_string(),
                    data_type: dt.describe(),
                })
            }
        };
        let i = || -> Result<i128, ValueError> {
            match (int_in, numeric_in) {
                (Some(i), _) => Ok(i),
                (None, Some(f)) => as_int(f),
                _ => Err(mismatch(dt, v.kind())),
            }
        };
        match dt {
            DataType::Int8 => int_from!(Int8, i8, i()?, dt),
            DataType::Int16 => int_from!(Int16, i16, i()?, dt),
            DataType::Int32 => int_from!(Int32, i32, i()?, dt),
            DataType::Int64 => int_from!(Int64, i64, i()?, dt),
            DataType::UInt8 => int_from!(UInt8, u8, i()?, dt),
            DataType::UInt16 => int_from!(UInt16, u16, i()?, dt),
            DataType::UInt32 => int_from!(UInt32, u32, i()?, dt),
            DataType::UInt64 => int_from!(UInt64, u64, i()?, dt),
            DataType::Float32 => float32(numeric_in.ok_or_else(|| mismatch(dt, v.kind()))?, dt),
            DataType::Float64 => {
                let f = numeric_in.ok_or_else(|| mismatch(dt, v.kind()))?;
                if f.is_finite() {
                    Ok(TagValue::Float64(f))
                } else {
                    Err(ValueError::NonFinite)
                }
            }
            _ => {
                self.check(&v, dt)?;
                Ok(v)
            }
        }
    }

    /// Type of the member at `path` inside a value of type `dt`.
    pub fn member_type(&self, dt: &DataType, path: &MemberPath) -> Result<DataType, ValueError> {
        let mut cur = dt.clone();
        for seg in &path.0 {
            cur = match (seg, &cur) {
                (Segment::Field(f), DataType::Struct(id)) => {
                    let st = self.struct_type(id)?;
                    st.members
                        .iter()
                        .find(|m| &m.name == f)
                        .map(|m| m.data_type.clone())
                        .ok_or_else(|| ValueError::UnknownMember {
                            member: f.clone(),
                            data_type: cur.describe(),
                        })?
                }
                (Segment::Index(i), DataType::Array { element, length }) => {
                    if *i >= *length {
                        return Err(ValueError::IndexOutOfBounds {
                            index: *i,
                            length: *length,
                        });
                    }
                    (**element).clone()
                }
                (Segment::Field(f), other) => {
                    return Err(ValueError::UnknownMember {
                        member: f.clone(),
                        data_type: other.describe(),
                    })
                }
                (Segment::Index(_), other) => {
                    return Err(mismatch(other, "array index"));
                }
            };
        }
        Ok(cur)
    }
}

fn float32(f: f64, dt: &DataType) -> Result<TagValue, ValueError> {
    if !f.is_finite() {
        return Err(ValueError::NonFinite);
    }
    if f.abs() > f64::from(f32::MAX) {
        return Err(ValueError::OutOfRange {
            value: f.to_string(),
            data_type: dt.describe(),
        });
    }
    Ok(TagValue::Float32(f as f32))
}
