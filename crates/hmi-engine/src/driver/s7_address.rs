//! S7 absolute addresses and their typed encoding.
//!
//! Syntax (case-insensitive): `<area>.<type><byte offset>[.<bit> | .<max length>]`
//!
//! | Area | Meaning |
//! | --- | --- |
//! | `DB<n>` | data block n (must have "optimized block access" **off**) |
//! | `M` | memory (merker) |
//! | `I` or `E` | process inputs |
//! | `Q` or `A` | process outputs |
//!
//! | Type | S7 type | Size |
//! | --- | --- | --- |
//! | `X<byte>.<bit>` | BOOL | bit |
//! | `BYTE`, `USINT` / `SINT` | 8-bit unsigned / signed | 1 |
//! | `WORD`, `UINT` / `INT` | 16-bit unsigned / signed | 2 |
//! | `DWORD`, `UDINT` / `DINT` | 32-bit unsigned / signed | 4 |
//! | `REAL` / `LREAL` | IEEE 754 32 / 64-bit | 4 / 8 |
//! | `STRING<byte>.<max>` | S7 STRING with max length (1–254) | max + 2 |
//!
//! Examples: `DB10.REAL4`, `DB10.INT8`, `DB10.X12.3`, `DB5.STRING20.32`, `M.INT100`, `I.X0.0`.
//! Shorthands for bits: `DB10.DBX12.3`, `M10.3`, `I0.0`, `Q4.1` (and `E`/`A`).
//!
//! The TIA forms `DBB`, `DBW`, `DBD`, `MB`, `MW`, `MD`, ... are refused on purpose: they say how
//! wide the access is but not whether it is signed, integer or REAL, and guessing wrong turns
//! −1 into 65535 on an operator screen.

use crate::protocol::{Value, ValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Area {
    Db(u16),
    Merker,
    Input,
    Output,
}

impl Area {
    /// rust7 area code.
    pub fn code(self) -> u8 {
        match self {
            Area::Db(_) => rust7::client::S7_AREA_DB,
            Area::Merker => rust7::client::S7_AREA_MK,
            Area::Input => rust7::client::S7_AREA_PE,
            Area::Output => rust7::client::S7_AREA_PA,
        }
    }

    pub fn db_number(self) -> u16 {
        match self {
            Area::Db(n) => n,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S7Type {
    Bool { bit: u8 },
    Byte,
    Usint,
    Sint,
    Word,
    Uint,
    Int,
    Dword,
    Udint,
    Dint,
    Real,
    Lreal,
    String { max_len: u8 },
}

impl S7Type {
    /// Bytes occupied in the PLC (a BOOL occupies its byte for reading).
    pub fn size(self) -> u32 {
        match self {
            S7Type::Bool { .. } | S7Type::Byte | S7Type::Usint | S7Type::Sint => 1,
            S7Type::Word | S7Type::Uint | S7Type::Int => 2,
            S7Type::Dword | S7Type::Udint | S7Type::Dint | S7Type::Real => 4,
            S7Type::Lreal => 8,
            S7Type::String { max_len } => max_len as u32 + 2,
        }
    }

    /// Value range of an integer type, if it is one.
    fn integer_range(self) -> Option<(i64, i64)> {
        Some(match self {
            S7Type::Byte | S7Type::Usint => (0, u8::MAX as i64),
            S7Type::Sint => (i8::MIN as i64, i8::MAX as i64),
            S7Type::Word | S7Type::Uint => (0, u16::MAX as i64),
            S7Type::Int => (i16::MIN as i64, i16::MAX as i64),
            S7Type::Dword | S7Type::Udint => (0, u32::MAX as i64),
            S7Type::Dint => (i32::MIN as i64, i32::MAX as i64),
            _ => return None,
        })
    }

    /// Whether every value of this S7 type can be represented by the HMI variable type, so
    /// reading can never wrap or truncate.
    pub fn readable_as(self, value_type: ValueType) -> bool {
        match (value_type, self) {
            (ValueType::Bool, S7Type::Bool { .. }) => true,
            (ValueType::Int, t) => t
                .integer_range()
                .is_some_and(|(lo, hi)| lo >= i32::MIN as i64 && hi <= i32::MAX as i64),
            (ValueType::Long, t) => t.integer_range().is_some(),
            (ValueType::Float, S7Type::Real) => true,
            (ValueType::Double, S7Type::Real | S7Type::Lreal) => true,
            (ValueType::String, S7Type::String { .. }) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct S7Address {
    pub area: Area,
    /// Byte offset in the area.
    pub offset: u32,
    pub kind: S7Type,
}

/// rust7 takes byte offsets as u16 and bit addresses (byte * 8 + bit) as u16.
const MAX_BYTE_OFFSET: u32 = u16::MAX as u32;
const MAX_BIT_BYTE_OFFSET: u32 = (u16::MAX as u32) / 8;

impl S7Address {
    pub fn parse(text: &str) -> Result<Self, String> {
        let original = text.trim();
        let upper = original.to_ascii_uppercase();
        let invalid = |why: &str| format!("'{original}' is not a valid S7 address: {why}");

        // Bit shorthands without a type token.
        if let Some(address) = Self::parse_bit_shorthand(&upper) {
            return address.map_err(|why| invalid(&why));
        }

        let (area_text, rest) = upper
            .split_once('.')
            .ok_or_else(|| invalid("expected <area>.<type><offset>, e.g. DB10.REAL4"))?;
        let area = match area_text {
            "M" => Area::Merker,
            "I" | "E" => Area::Input,
            "Q" | "A" => Area::Output,
            db if db.starts_with("DB") => {
                let n: u16 = db[2..].parse().map_err(|_| invalid("bad DB number"))?;
                if n == 0 {
                    return Err(invalid("DB numbers start at 1"));
                }
                Area::Db(n)
            }
            _ => return Err(invalid("area must be DB<n>, M, I/E or Q/A")),
        };

        for ambiguous in ["DBB", "DBW", "DBD"] {
            if rest.starts_with(ambiguous) && rest[3..].starts_with(|c: char| c.is_ascii_digit()) {
                return Err(invalid(&format!(
                    "{ambiguous} does not say the data type; write e.g. {}.INT{} / {}.WORD{} / {}.REAL{}",
                    area_text,
                    &rest[3..],
                    area_text,
                    &rest[3..],
                    area_text,
                    &rest[3..]
                )));
            }
        }
        let rest = rest
            .strip_prefix("DBX")
            .map(|r| format!("X{r}"))
            .unwrap_or_else(|| rest.to_owned());

        let split = rest
            .find(|c: char| c.is_ascii_digit())
            .ok_or_else(|| invalid("missing byte offset"))?;
        let (type_text, numbers) = rest.split_at(split);
        let mut parts = numbers.split('.');
        let offset: u32 = parts
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| invalid("bad byte offset"))?;
        let extra = parts.next();
        if parts.next().is_some() {
            return Err(invalid("too many parts"));
        }
        let extra_number = |what: &str| -> Result<u32, String> {
            extra
                .ok_or_else(|| invalid(&format!("missing {what}")))?
                .parse::<u32>()
                .map_err(|_| invalid(&format!("bad {what}")))
        };
        let no_extra = || -> Result<(), String> {
            match extra {
                Some(_) => Err(invalid("unexpected '.<number>' for this type")),
                None => Ok(()),
            }
        };

        let kind = match type_text {
            "X" => {
                let bit = extra_number("bit number")?;
                if bit > 7 {
                    return Err(invalid("bit number must be 0..7"));
                }
                S7Type::Bool { bit: bit as u8 }
            }
            "BYTE" | "B" => no_extra().map(|_| S7Type::Byte)?,
            "USINT" => no_extra().map(|_| S7Type::Usint)?,
            "SINT" => no_extra().map(|_| S7Type::Sint)?,
            "WORD" => no_extra().map(|_| S7Type::Word)?,
            "UINT" => no_extra().map(|_| S7Type::Uint)?,
            "INT" => no_extra().map(|_| S7Type::Int)?,
            "DWORD" => no_extra().map(|_| S7Type::Dword)?,
            "UDINT" => no_extra().map(|_| S7Type::Udint)?,
            "DINT" => no_extra().map(|_| S7Type::Dint)?,
            "REAL" => no_extra().map(|_| S7Type::Real)?,
            "LREAL" => no_extra().map(|_| S7Type::Lreal)?,
            "STRING" => {
                let max = extra_number("maximum length (STRING<offset>.<max>)")?;
                if !(1..=254).contains(&max) {
                    return Err(invalid("STRING maximum length must be 1..254"));
                }
                S7Type::String { max_len: max as u8 }
            }
            "W" | "D" | "DB" | "MB" | "MW" | "MD" => {
                return Err(invalid(
                    "width-only types are ambiguous; use INT, WORD, DINT, DWORD or REAL",
                ))
            }
            other => return Err(invalid(&format!("unknown type '{other}'"))),
        };
        Self::checked(area, offset, kind).map_err(|why| invalid(&why))
    }

    /// `M10.3`, `I0.0`, `E0.0`, `Q4.1`, `A4.1`.
    fn parse_bit_shorthand(upper: &str) -> Option<Result<Self, String>> {
        let (first, rest) = upper.split_at(1.min(upper.len()));
        let area = match first {
            "M" => Area::Merker,
            "I" | "E" => Area::Input,
            "Q" | "A" => Area::Output,
            _ => return None,
        };
        if !rest.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        let (byte, bit) = rest.split_once('.')?;
        let parsed = (|| {
            let offset: u32 = byte.parse().map_err(|_| "bad byte offset".to_owned())?;
            let bit: u8 = bit.parse().map_err(|_| "bad bit number".to_owned())?;
            if bit > 7 {
                return Err("bit number must be 0..7".to_owned());
            }
            Self::checked(area, offset, S7Type::Bool { bit })
        })();
        Some(parsed)
    }

    fn checked(area: Area, offset: u32, kind: S7Type) -> Result<Self, String> {
        let limit = if matches!(kind, S7Type::Bool { .. }) {
            MAX_BIT_BYTE_OFFSET
        } else {
            MAX_BYTE_OFFSET
        };
        if offset + kind.size() - 1 > limit {
            return Err(format!(
                "offset {offset} is beyond the supported range (max {limit})"
            ));
        }
        Ok(Self { area, offset, kind })
    }

    /// Byte range `[start, end)` this address occupies.
    pub fn bytes(&self) -> (u32, u32) {
        (self.offset, self.offset + self.kind.size())
    }

    /// Decodes this address's value from `data`, which starts at byte `base` of the area.
    pub fn decode(&self, data: &[u8], base: u32, wanted: ValueType) -> Result<Value, &'static str> {
        let start = (self.offset - base) as usize;
        let bytes = data
            .get(start..start + self.kind.size() as usize)
            .ok_or("ShortRead")?;
        let int: Option<i64> = match self.kind {
            S7Type::Byte | S7Type::Usint => Some(bytes[0] as i64),
            S7Type::Sint => Some(bytes[0] as i8 as i64),
            S7Type::Word | S7Type::Uint => Some(u16::from_be_bytes([bytes[0], bytes[1]]) as i64),
            S7Type::Int => Some(i16::from_be_bytes([bytes[0], bytes[1]]) as i64),
            S7Type::Dword | S7Type::Udint => {
                Some(u32::from_be_bytes(bytes[..4].try_into().unwrap()) as i64)
            }
            S7Type::Dint => Some(i32::from_be_bytes(bytes[..4].try_into().unwrap()) as i64),
            _ => None,
        };
        match (self.kind, wanted, int) {
            (S7Type::Bool { bit }, ValueType::Bool, _) => {
                Ok(Value::Bool(bytes[0] & (1 << bit) != 0))
            }
            (_, ValueType::Int, Some(v)) => {
                i32::try_from(v).map(Value::Int).map_err(|_| "OutOfRange")
            }
            (_, ValueType::Long, Some(v)) => Ok(Value::Long(v)),
            (S7Type::Real, ValueType::Float, _) => Ok(Value::Float(f32::from_be_bytes(
                bytes[..4].try_into().unwrap(),
            ))),
            (S7Type::Real, ValueType::Double, _) => Ok(Value::Double(f32::from_be_bytes(
                bytes[..4].try_into().unwrap(),
            ) as f64)),
            (S7Type::Lreal, ValueType::Double, _) => Ok(Value::Double(f64::from_be_bytes(
                bytes[..8].try_into().unwrap(),
            ))),
            (S7Type::String { max_len }, ValueType::String, _) => {
                // S7 STRING: [declared max][current length][characters...]
                let current = (bytes[1] as usize)
                    .min(max_len as usize)
                    .min(bytes[0] as usize);
                // Single-byte Siemens character set: Latin-1 maps 1:1 to Unicode.
                Ok(Value::String(
                    bytes[2..2 + current].iter().map(|&b| b as char).collect(),
                ))
            }
            _ => Err("TypeMismatch"),
        }
    }

    /// Encodes a value for writing. Refuses anything that does not fit exactly.
    /// BOOL is written as a single bit, so it is not encoded here.
    pub fn encode(&self, value: &Value) -> Result<Vec<u8>, String> {
        let out_of_range =
            |v: &dyn std::fmt::Display| format!("value {v} does not fit {:?}", self.kind);
        if let Some((lo, hi)) = self.kind.integer_range() {
            let v: i64 = match value {
                Value::Int(v) => *v as i64,
                Value::Long(v) => *v,
                other => {
                    return Err(format!(
                        "a {:?} value cannot be written to {:?}",
                        other.value_type(),
                        self.kind
                    ))
                }
            };
            if v < lo || v > hi {
                return Err(out_of_range(&v));
            }
            return Ok(match self.kind.size() {
                1 => vec![v as u8],
                2 => (v as u16).to_be_bytes().to_vec(),
                _ => (v as u32).to_be_bytes().to_vec(),
            });
        }
        match (self.kind, value) {
            (S7Type::Real, Value::Float(v)) => Ok(v.to_be_bytes().to_vec()),
            (S7Type::Real, Value::Double(v)) => {
                if v.abs() > f32::MAX as f64 {
                    Err(out_of_range(v))
                } else {
                    Ok((*v as f32).to_be_bytes().to_vec())
                }
            }
            (S7Type::Lreal, Value::Double(v)) => Ok(v.to_be_bytes().to_vec()),
            (S7Type::String { max_len }, Value::String(text)) => {
                let mut bytes = Vec::with_capacity(text.len());
                for c in text.chars() {
                    let code = c as u32;
                    if code > 0xFF {
                        return Err(format!("character '{c}' cannot be stored in an S7 STRING"));
                    }
                    bytes.push(code as u8);
                }
                if bytes.len() > max_len as usize {
                    return Err(format!(
                        "text is {} characters; the STRING holds {max_len}",
                        bytes.len()
                    ));
                }
                // Header + text only: bytes after the current length are left as they are.
                let mut out = vec![max_len, bytes.len() as u8];
                out.extend(bytes);
                Ok(out)
            }
            (kind, value) => Err(format!(
                "a {:?} value cannot be written to {kind:?}",
                value.value_type()
            )),
        }
    }
}

/// Contiguous byte ranges to read per poll, covering every readable point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSpan {
    pub area: Area,
    pub start: u32,
    pub end: u32,
}

/// Merges points of the same area that are at most `max_gap` bytes apart into one read,
/// without letting a span grow beyond `max_len` bytes.
pub fn plan_reads(addresses: &[S7Address], max_gap: u32, max_len: u32) -> Vec<ReadSpan> {
    let mut sorted: Vec<&S7Address> = addresses.iter().collect();
    sorted.sort_by_key(|a| (a.area, a.offset));
    let mut spans: Vec<ReadSpan> = Vec::new();
    for address in sorted {
        let (start, end) = address.bytes();
        if let Some(last) = spans.last_mut() {
            if last.area == address.area
                && start <= last.end + max_gap
                && end.max(last.end) - last.start <= max_len
            {
                last.end = last.end.max(end);
                continue;
            }
        }
        spans.push(ReadSpan {
            area: address.area,
            start,
            end,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> S7Address {
        S7Address::parse(text).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn parses_the_documented_forms() {
        assert_eq!(
            parse("DB10.REAL4"),
            S7Address {
                area: Area::Db(10),
                offset: 4,
                kind: S7Type::Real
            }
        );
        assert_eq!(parse("db10.int8").kind, S7Type::Int);
        assert_eq!(
            parse("DB10.X12.3"),
            S7Address {
                area: Area::Db(10),
                offset: 12,
                kind: S7Type::Bool { bit: 3 }
            }
        );
        assert_eq!(parse("DB10.DBX12.3"), parse("DB10.X12.3"));
        assert_eq!(
            parse("DB5.STRING20.32").kind,
            S7Type::String { max_len: 32 }
        );
        assert_eq!(
            parse("M.INT100"),
            S7Address {
                area: Area::Merker,
                offset: 100,
                kind: S7Type::Int
            }
        );
        assert_eq!(
            parse("M10.3"),
            S7Address {
                area: Area::Merker,
                offset: 10,
                kind: S7Type::Bool { bit: 3 }
            }
        );
        assert_eq!(parse("E0.0").area, Area::Input);
        assert_eq!(parse("A4.1").area, Area::Output);
        assert_eq!(parse("Q.DINT0").kind, S7Type::Dint);
        assert_eq!(parse("I.X0.7").kind, S7Type::Bool { bit: 7 });
        for (text, kind) in [
            ("DB1.BYTE0", S7Type::Byte),
            ("DB1.USINT0", S7Type::Usint),
            ("DB1.SINT0", S7Type::Sint),
            ("DB1.WORD0", S7Type::Word),
            ("DB1.UINT0", S7Type::Uint),
            ("DB1.DWORD0", S7Type::Dword),
            ("DB1.UDINT0", S7Type::Udint),
            ("DB1.DINT0", S7Type::Dint),
            ("DB1.LREAL0", S7Type::Lreal),
        ] {
            assert_eq!(parse(text).kind, kind, "{text}");
        }
    }

    #[test]
    fn refuses_ambiguous_and_malformed_addresses() {
        for bad in [
            "DB10.DBW4",
            "DB10.DBD4",
            "DB10.DBB4",
            "DB10.W4",
            "DB0.INT0",
            "DB10.X4",
            "DB10.X4.8",
            "DB10.INT4.1",
            "DB10.STRING4",
            "DB10.STRING4.300",
            "DB10.FOO4",
            "X10.INT4",
            "DB10",
            "DB10.INT",
            "M10.9",
            "DB10.INT65535",
            "DB10.X9000.0",
            "",
        ] {
            assert!(S7Address::parse(bad).is_err(), "should be refused: {bad}");
        }
        let hint = S7Address::parse("DB10.DBW4").unwrap_err();
        assert!(
            hint.contains("DB10.INT4") && hint.contains("DB10.WORD4"),
            "{hint}"
        );
    }

    #[test]
    fn only_type_pairs_that_can_never_overflow_are_readable() {
        assert!(S7Type::Int.readable_as(ValueType::Int));
        assert!(S7Type::Dint.readable_as(ValueType::Int));
        assert!(
            !S7Type::Udint.readable_as(ValueType::Int),
            "UDINT can exceed i32"
        );
        assert!(!S7Type::Dword.readable_as(ValueType::Int));
        assert!(S7Type::Udint.readable_as(ValueType::Long));
        assert!(S7Type::Real.readable_as(ValueType::Float));
        assert!(S7Type::Real.readable_as(ValueType::Double));
        assert!(
            !S7Type::Lreal.readable_as(ValueType::Float),
            "LREAL loses precision as FLOAT"
        );
        assert!(!S7Type::Int.readable_as(ValueType::Float));
        assert!(!S7Type::Real.readable_as(ValueType::Int));
        assert!(S7Type::Bool { bit: 0 }.readable_as(ValueType::Bool));
        assert!(!S7Type::Byte.readable_as(ValueType::Bool));
    }

    #[test]
    fn decodes_big_endian_values() {
        let data = [
            0x00,
            0x00,
            0b0000_1000,
            0x00, // X2.3 set
            0xFF,
            0xFF, // INT4 = -1 / WORD4 = 65535
            0x42,
            0x2A,
            0x00,
            0x00, // REAL6 = 42.5
            0xFF,
            0xFF,
            0xFF,
            0xFE, // DINT10 = -2
            0x05,
            0x02,
            b'O',
            b'K',
            b'x',
            b'x',
            b'x', // STRING14.5 = "OK"
        ];
        let at = |text: &str, wanted| parse(text).decode(&data, 0, wanted);
        assert_eq!(at("DB1.X2.3", ValueType::Bool), Ok(Value::Bool(true)));
        assert_eq!(at("DB1.X2.2", ValueType::Bool), Ok(Value::Bool(false)));
        assert_eq!(at("DB1.INT4", ValueType::Int), Ok(Value::Int(-1)));
        assert_eq!(at("DB1.WORD4", ValueType::Int), Ok(Value::Int(65535)));
        assert_eq!(at("DB1.REAL6", ValueType::Float), Ok(Value::Float(42.5)));
        assert_eq!(at("DB1.DINT10", ValueType::Long), Ok(Value::Long(-2)));
        assert_eq!(
            at("DB1.UDINT10", ValueType::Long),
            Ok(Value::Long(0xFFFF_FFFE))
        );
        assert_eq!(
            at("DB1.STRING14.5", ValueType::String),
            Ok(Value::String("OK".into()))
        );
        // Offsets relative to the span start.
        assert_eq!(
            parse("DB1.INT4").decode(&data[4..], 4, ValueType::Int),
            Ok(Value::Int(-1))
        );
        assert_eq!(
            parse("DB1.LREAL16").decode(&data, 0, ValueType::Double),
            Err("ShortRead")
        );
    }

    #[test]
    fn encodes_exactly_or_refuses() {
        assert_eq!(
            parse("DB1.INT0").encode(&Value::Int(-2)),
            Ok(vec![0xFF, 0xFE])
        );
        assert_eq!(
            parse("DB1.DINT0").encode(&Value::Int(1200)),
            Ok(vec![0, 0, 0x04, 0xB0])
        );
        assert_eq!(
            parse("DB1.UDINT0").encode(&Value::Long(0xFFFF_FFFF)),
            Ok(vec![0xFF; 4])
        );
        assert_eq!(
            parse("DB1.REAL0").encode(&Value::Float(42.5)),
            Ok(vec![0x42, 0x2A, 0, 0])
        );
        assert_eq!(
            parse("DB1.LREAL0").encode(&Value::Double(1.0)),
            Ok(1.0f64.to_be_bytes().to_vec())
        );
        assert_eq!(
            parse("DB1.STRING0.5").encode(&Value::String("Hé".into())),
            Ok(vec![5, 2, b'H', 0xE9])
        );
        assert!(parse("DB1.INT0").encode(&Value::Int(40_000)).is_err());
        assert!(parse("DB1.UINT0").encode(&Value::Int(-1)).is_err());
        assert!(parse("DB1.SINT0").encode(&Value::Int(128)).is_err());
        assert!(parse("DB1.UDINT0").encode(&Value::Long(1 << 32)).is_err());
        assert!(parse("DB1.REAL0").encode(&Value::Double(1e300)).is_err());
        assert!(parse("DB1.REAL0").encode(&Value::Int(1)).is_err());
        assert!(parse("DB1.STRING0.3")
            .encode(&Value::String("toolong".into()))
            .is_err());
        assert!(parse("DB1.STRING0.9")
            .encode(&Value::String("€".into()))
            .is_err());
    }

    #[test]
    fn reads_are_grouped_into_few_contiguous_spans() {
        let addresses: Vec<S7Address> = [
            "DB1.INT0",
            "DB1.REAL4",
            "DB1.X9.1",
            "DB1.INT200",
            "DB2.INT0",
            "M.INT0",
        ]
        .iter()
        .map(|t| parse(t))
        .collect();
        let spans = plan_reads(&addresses, 16, 512);
        assert_eq!(
            spans,
            vec![
                ReadSpan {
                    area: Area::Db(1),
                    start: 0,
                    end: 10
                },
                ReadSpan {
                    area: Area::Db(1),
                    start: 200,
                    end: 202
                },
                ReadSpan {
                    area: Area::Db(2),
                    start: 0,
                    end: 2
                },
                ReadSpan {
                    area: Area::Merker,
                    start: 0,
                    end: 2
                },
            ]
        );
        let long: Vec<S7Address> = (0..100)
            .map(|i| parse(&format!("DB1.INT{}", i * 10)))
            .collect();
        assert!(plan_reads(&long, 16, 512)
            .iter()
            .all(|s| s.end - s.start <= 512));
    }

    #[test]
    fn every_address_form_the_tag_importers_emit_is_accepted() {
        // Keep in step with core/tagimport/TiaSourceImporter.kt.
        for text in [
            "DB10.X0.0",
            "DB10.BYTE8",
            "DB10.USINT8",
            "DB10.SINT8",
            "DB10.WORD2",
            "DB10.UINT2",
            "DB10.INT2",
            "DB10.DWORD4",
            "DB10.UDINT10",
            "DB10.DINT4",
            "DB10.REAL4",
            "DB10.LREAL36",
            "DB10.STRING14.20",
            "M10.3",
            "I0.0",
            "Q4.1",
            "M.INT100",
            "M.REAL20",
            "I.WORD64",
            "Q.BYTE0",
        ] {
            parse(text);
        }
    }
}
