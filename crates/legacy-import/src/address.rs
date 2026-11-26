//! Bounded, declaration-only address admission. This is a deliberately
//! narrower subset of the engine address vocabulary, not a device probe.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Span {
    pub area: String,
    pub start: u32,
    pub end: u32,
    pub integer_range: Option<(i64, i64)>,
}

fn decimal(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

pub(super) fn s7(address: &str, ty: &str, writable: bool) -> Option<Span> {
    if !address.is_ascii() || address.len() > 80 {
        return None;
    }
    let upper = address.to_ascii_uppercase();
    // Normalize the engine's M/I/E/Q/A shorthand bit syntax before parsing.
    let shorthand;
    let upper = if upper.as_bytes().first().is_some_and(|b| b"MIEQA".contains(b))
        && upper.as_bytes().get(1).is_some_and(u8::is_ascii_digit)
    {
        shorthand = format!("{}.X{}", &upper[..1], &upper[1..]);
        shorthand.as_str()
    } else {
        upper.as_str()
    };
    let (area, rest) = upper.split_once('.')?;
    let area = match area {
        "M" => "M".to_owned(),
        "I" | "E" if !writable => "I".to_owned(),
        "Q" | "A" => "Q".to_owned(),
        db if db.starts_with("DB") => {
            let n = decimal(&db[2..])?;
            if !(1..=65535).contains(&n) {
                return None;
            }
            format!("DB{n}")
        }
        _ => return None,
    };
    let pos = rest.find(|c: char| c.is_ascii_digit())?;
    let (kind, numbers) = rest.split_at(pos);
    let (offset, extra) = match numbers.split_once('.') {
        Some((a, b)) => (decimal(a)?, Some(decimal(b)?)),
        None => (decimal(numbers)?, None),
    };
    let size = match (kind, ty, extra) {
        ("X" | "DBX", "bool", Some(bit)) if bit <= 7 && offset <= 8191 => {
            // Treat the containing byte as occupied, conservatively refusing
            // independent writable bit aliases in the same byte.
            1
        }
        ("BYTE" | "B" | "USINT" | "SINT", "int32" | "int64", None) => 1,
        ("WORD" | "UINT" | "INT", "int32" | "int64", None) => 2,
        ("DINT", "int32" | "int64", None) => 4,
        ("DWORD" | "UDINT", "int64", None) => 4,
        ("REAL", "float32", None) => 4,
        ("REAL", "float64", None) if !writable => 4,
        ("LREAL", "float64", None) => 8,
        ("STRING", "string", Some(n)) if (1..=254).contains(&n) && !writable => n + 2,
        _ => return None,
    };
    let integer_range = match kind {
        "BYTE" | "B" | "USINT" => Some((0, 255)),
        "SINT" => Some((-128, 127)),
        "WORD" | "UINT" => Some((0, 65535)),
        "INT" => Some((-32768, 32767)),
        "DINT" => Some((i64::from(i32::MIN), i64::from(i32::MAX))),
        "DWORD" | "UDINT" => Some((0, i64::from(u32::MAX))),
        _ => None,
    };
    let end = offset.checked_add(size)?;
    (end <= 65536).then_some(Span {
        area,
        start: offset,
        end,
        integer_range,
    })
}

pub(super) fn opcua(address: &str) -> Option<String> {
    if address.len() > 1024 || address.chars().any(char::is_control) {
        return None;
    }
    let (namespace, identifier) = address.strip_prefix("ns=")?.split_once(';')?;
    let ns = u16::try_from(decimal(namespace)?).ok()?;
    let (kind, value) = identifier.split_once('=')?;
    let normalized = match kind {
        "i" => decimal(value)?.to_string(),
        "s" if !value.is_empty() && value.trim() == value => value.to_owned(),
        _ => return None,
    };
    Some(format!("ns={ns};{kind}={normalized}"))
}
