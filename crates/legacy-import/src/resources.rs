//! Integrity-checked in-memory resource transfer; no package or filesystem I/O.
use super::{path, Mapping};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn digit(b: u8) -> Option<u8> {
    match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut chunks = s.as_bytes().as_chunks::<4>().0.iter().peekable();
    while let Some(c) = chunks.next() {
        let a = digit(c[0])?;
        let b = digit(c[1])?;
        out.push(a << 2 | b >> 4);
        if c[2] == b'=' {
            if c[3] != b'=' || b & 15 != 0 || chunks.peek().is_some() {
                return None;
            }
            break;
        }
        let d = digit(c[2])?;
        out.push(b << 4 | d >> 2);
        if c[3] == b'=' {
            if d & 3 != 0 || chunks.peek().is_some() {
                return None;
            }
            break;
        }
        out.push(d << 6 | digit(c[3])?);
    }
    Some(out)
}

// Verify the complete PNG chunk envelope, CRCs and declared dimensions.
// This is not pixel decoding or decompression-bomb validation by a renderer.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut rest = &bytes[8..];
    let mut dimensions = None;
    let mut idat = false;
    while !rest.is_empty() {
        let len = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
        let end = len.checked_add(8)?;
        let data = rest.get(4..end)?;
        let crc_end = end.checked_add(4)?;
        let expected = u32::from_be_bytes(rest.get(end..crc_end)?.try_into().ok()?);
        let mut crc = !0u32;
        for b in data {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        if !crc != expected {
            return None;
        }
        let kind = &data[..4];
        if dimensions.is_none() && kind != b"IHDR" {
            return None;
        }
        match kind {
            b"IHDR" => {
                if dimensions.is_some() || len != 13 {
                    return None;
                }
                let w = u32::from_be_bytes(data[4..8].try_into().ok()?);
                let h = u32::from_be_bytes(data[8..12].try_into().ok()?);
                if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 16_000_000 {
                    return None;
                }
                dimensions = Some((w, h));
            }
            b"IDAT" => idat = true,
            b"IEND" => {
                return if len == 0 && idat && crc_end == rest.len() {
                    dimensions
                } else {
                    None
                };
            }
            _ => {}
        }
        rest = rest.get(crc_end..)?;
    }
    None
}

// Conservative baseline JPEG envelope: one sequential scan, bounded
// dimensions, marker lengths and a terminal EOI. No entropy/pixel decoding.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut rest = bytes.strip_prefix(&[0xff, 0xd8])?;
    let mut dimensions = None;
    let mut components = 0;
    while !rest.is_empty() {
        if rest.first() != Some(&0xff) {
            return None;
        }
        let marker = *rest.get(1)?;
        rest = rest.get(2..)?;
        let len = usize::from(u16::from_be_bytes(rest.get(..2)?.try_into().ok()?));
        if len < 2 {
            return None;
        }
        let data = rest.get(2..len)?;
        rest = rest.get(len..)?;
        match marker {
            0xc0 => {
                if dimensions.is_some() || data.first() != Some(&8) {
                    return None;
                }
                let h = u32::from(u16::from_be_bytes(data.get(1..3)?.try_into().ok()?));
                let w = u32::from(u16::from_be_bytes(data.get(3..5)?.try_into().ok()?));
                components = usize::from(*data.get(5)?);
                if !matches!(components, 1 | 3)
                    || data.len() != 6 + 3 * components
                    || w == 0
                    || h == 0
                    || u64::from(w) * u64::from(h) > 16_000_000
                {
                    return None;
                }
                dimensions = Some((w, h));
            }
            0xda => {
                let size = dimensions?;
                if usize::from(*data.first()?) != components
                    || data.len() != 4 + 2 * components
                    || data.get(data.len() - 3..) != Some(&[0, 63, 0])
                {
                    return None;
                }
                let mut i = 0;
                while i < rest.len() {
                    if rest[i] != 0xff {
                        i += 1;
                        continue;
                    }
                    match *rest.get(i + 1)? {
                        0x00 | 0xd0..=0xd7 => i += 2,
                        0xd9 if i > 0 && i + 2 == rest.len() => return Some(size),
                        _ => return None,
                    }
                }
                return None;
            }
            0xe0 if data.starts_with(b"JFIF\0") => {}
            0xdb | 0xc4 | 0xdd => {}
            _ => return None,
        }
    }
    None
}

// Parse only a deliberately small, non-extensible static SVG grammar.
// No entity expansion, namespaces other than the literal root SVG namespace,
// scripts, styles, references, nested elements or processing instructions.
fn attributes(mut s: &str) -> Option<BTreeMap<&str, &str>> {
    let mut attrs = BTreeMap::new();
    while !s.is_empty() {
        if !s.starts_with(' ') {
            return None;
        }
        s = s.trim_start_matches(' ');
        if s.is_empty() {
            break;
        }
        let (key, tail) = s.split_once("=\"")?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphabetic()) {
            return None;
        }
        let (value, tail) = tail.split_once('"')?;
        if value.contains(['<', '>', '&']) || attrs.insert(key, value).is_some() {
            return None;
        }
        s = tail;
    }
    Some(attrs)
}

fn positive_integer(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = s.parse::<u32>().ok()?;
    (n <= 4096).then_some(n)
}

fn svg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let xml = std::str::from_utf8(bytes).ok()?.trim();
    if !xml.is_ascii() || xml.len() > 256 * 1024 {
        return None;
    }
    let root = xml.strip_prefix("<svg")?;
    let (header, body) = root.split_once('>')?;
    let mut attrs = attributes(header)?;
    if attrs.remove("xmlns")? != "http://www.w3.org/2000/svg" {
        return None;
    }
    let w = positive_integer(attrs.remove("width")?)?;
    let h = positive_integer(attrs.remove("height")?)?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 16_000_000 {
        return None;
    }
    if let Some(view_box) = attrs.remove("viewBox") {
        if view_box != format!("0 0 {w} {h}") {
            return None;
        }
    }
    if !attrs.is_empty() {
        return None;
    }
    let mut body = body.strip_suffix("</svg>")?.trim();
    let mut count = 0;
    while !body.is_empty() {
        let (element, tail) = body.split_once("/>")?;
        let mut attrs = attributes(element.strip_prefix("<rect")?)?;
        let x = positive_integer(attrs.remove("x")?)?;
        let y = positive_integer(attrs.remove("y")?)?;
        let width = positive_integer(attrs.remove("width")?)?;
        let height = positive_integer(attrs.remove("height")?)?;
        let fill = attrs.remove("fill")?;
        if width == 0
            || height == 0
            || x + width > w
            || y + height > h
            || fill.len() != 7
            || !fill.starts_with('#')
            || !fill[1..].bytes().all(|b| b.is_ascii_hexdigit())
            || !attrs.is_empty()
        {
            return None;
        }
        count += 1;
        if count > 1024 {
            return None;
        }
        body = tail.trim();
    }
    (count > 0).then_some((w, h))
}

pub(super) fn map(v: &Value, doc: &mut Value, m: &mut Mapping) -> Option<()> {
    let Some(assets) = v.get("assets") else {
        return Some(());
    };
    let assets = assets.as_object()?;
    if assets.is_empty() {
        return Some(());
    }
    if v["schemaVersion"].as_u64()? < 7 {
        m.fail("/assets", "assets-version-mismatch");
        return None;
    }
    let mut resources = Vec::new();
    for (key, a) in assets {
        let p = path("/assets", key);
        if a["id"].as_str() != Some(key.as_str()) {
            m.fail(&p, "unsupported-asset-kind-or-id");
            return None;
        }
        let data = a.get("data")?.as_str()?;
        let bytes = if a["kind"] == "SVG" {
            data.as_bytes().to_vec()
        } else {
            decode(data)?
        };
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if a["sha256"].as_str() != Some(digest.as_str())
            || a["sizeBytes"].as_u64()? != bytes.len() as u64
        {
            m.fail(&p, "asset-integrity-mismatch");
            return None;
        }
        let (ext, media, kind, dimensions) = match a["kind"].as_str()? {
            "PNG" => ("png", "image/png", "image", png_dimensions(&bytes)),
            "JPEG" => ("jpg", "image/jpeg", "image", jpeg_dimensions(&bytes)),
            "SVG" => ("svg", "image/svg+xml", "vector", svg_dimensions(&bytes)),
            _ => {
                m.fail(&p, "unsupported-asset-kind-or-id");
                return None;
            }
        };
        let (w, h) = dimensions?;
        if a.get("widthPx").map(Value::as_f64).unwrap_or(Some(100.))? != f64::from(w)
            || a.get("heightPx").map(Value::as_f64).unwrap_or(Some(100.))? != f64::from(h)
        {
            m.fail(&p, "asset-dimension-mismatch");
            return None;
        }
        let id = m.id(&path(&p, "id"), key, "asset");
        let file = format!("assets/{digest}.{ext}");
        let t = format!("/resources/{}", resources.len());
        let name = m.text(
            a.get("name")?,
            &path(&p, "name"),
            &format!("{t}/extensions/legacyName"),
        )?;
        resources.push(json!({"id":id,"kind":kind,"path":file,"mediaType":media,"sha256":digest,"extensions":{"legacyName":name,"legacyWidthPx":w,"legacyHeightPx":h}}));
        for (src, dst) in [
            ("id", "id"),
            ("kind", "mediaType"),
            ("sha256", "sha256"),
            ("sizeBytes", "sha256"),
            ("widthPx", "extensions/legacyWidthPx"),
            ("heightPx", "extensions/legacyHeightPx"),
            ("data", "path"),
        ] {
            m.mark(&path(&p, src), &format!("{t}/{dst}"));
        }
        m.resource_bytes.insert(file, bytes);
    }
    doc["resources"] = json!(resources);
    Some(())
}

pub(super) fn image(
    c: &Value,
    n: &mut Value,
    p: &str,
    t: &str,
    m: &mut Mapping,
    version: u64,
    assets: &Value,
) -> Option<()> {
    if version < 7 {
        return None;
    }
    let props = c.get("properties")?.as_object()?;
    let old = props.get("assetId")?.as_str()?;
    if (c["type"] == "svgImage") != (assets[old]["kind"] == "SVG") {
        return None;
    }
    let id = m.ids.get(&path(&path("/assets", old), "id"))?.clone();
    let fit = match props
        .get("fit")
        .and_then(Value::as_str)
        .unwrap_or("CONTAIN")
    {
        "CONTAIN" => "fit",
        "COVER" => "crop",
        "STRETCH" => "fill",
        "CENTER" => "none",
        _ => return None,
    };
    n["properties"]["source"] = json!(id);
    n["properties"]["contentScale"] = json!(fit);
    m.mark(
        &format!("{p}/properties/assetId"),
        &format!("{t}/properties/source"),
    );
    m.mark(
        &format!("{p}/properties/fit"),
        &format!("{t}/properties/contentScale"),
    );
    if let Some(f) = m.findings.get_mut(&path(p, "type")) {
        f.code = "image-renderer-parity-pending";
        f.severity = "warning";
        f.message = "Verified resource bytes transferred; decoding and renderer parity pending.";
    }
    Some(())
}

// The frozen theme contract supports explicit named colors, but not the
// legacy equipment-state and component-default cascades. Those remain refused.
pub(super) fn theme(v: &Value, m: &mut Mapping) -> Option<Option<Value>> {
    let Some(theme) = v.get("theme").filter(|x| !x.is_null()) else {
        return Some(None);
    };
    if v["schemaVersion"].as_u64()? < 7 {
        m.fail("/theme", "theme-version-mismatch");
        return None;
    }
    let id = m.id("/theme/id", theme.get("id")?.as_str()?, "theme");
    let name = m.text(theme.get("name")?, "/theme/name", "/themes/0/name")?;
    let dark = theme
        .get("dark")
        .map(Value::as_bool)
        .unwrap_or(Some(false))?;
    m.mark("/theme/id", "/themes/0/id");
    m.mark("/theme/dark", "/themes/0/dark");
    if let Some(tokens) = theme.get("tokens") {
        let tokens = tokens.as_object()?;
        if tokens.is_empty() {
            m.mark("/theme/tokens", "/themes/0/colors");
        }
        for (key, value) in tokens {
            let s = value.as_str()?;
            if key.is_empty()
                || s.len() != 7
                || !s.starts_with('#')
                || !s[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                m.fail(&path("/theme/tokens", key), "unsupported-theme-color");
                return None;
            }
            m.theme_colors.insert(key.clone(), s.to_owned());
            m.mark(&path("/theme/tokens", key), &path("/themes/0/colors", key));
        }
    }
    styles(theme, m)?;
    if let Some(f) = m.findings.get_mut("/theme/id") {
        f.severity = "warning";
        f.code = "explicit-theme-colors-only";
        f.message =
            "Explicit colors mapped; implicit legacy renderer defaults are not established.";
    }
    Some(Some(
        json!({"id":id,"name":name,"dark":dark,"colors":m.theme_colors}),
    ))
}

// Common opacity is the only property shared by all supported component
// kinds. Shape-specific defaults and overrides are flattened at placement.
fn style_value(key: &str, value: &Value, m: &Mapping) -> Option<(&'static str, Value)> {
    match key {
        "fillColor" | "strokeColor" => {
            let s = value.as_str()?;
            let color = if let Some(token) = s.strip_prefix('@') {
                m.theme_colors.get(token)?;
                json!({"token":token})
            } else if s.len() == 7
                && s.starts_with('#')
                && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
            {
                json!(s)
            } else {
                return None;
            };
            Some((if key == "fillColor" { "fill" } else { "strokeColor" }, color))
        }
        "strokeWidthDp" | "cornerRadiusDp" | "opacity" => {
            let n = f64::from(value.as_f64()? as f32);
            let (target, min, max) = match key {
                "strokeWidthDp" => ("strokeWidth", 0., 32.),
                "cornerRadiusDp" => ("cornerRadius", 0., 64.),
                _ => ("opacity", 0.05, 1.),
            };
            (min..=max).contains(&n).then(|| (target, json!(n)))
        }
        _ => None,
    }
}

fn styles(theme: &Value, m: &mut Mapping) -> Option<()> {
    for group in ["styles", "componentDefaults"] {
        let Some(source) = theme.get(group) else {
            continue;
        };
        let source = source.as_object()?;
        let base = path("/theme", group);
        if source.is_empty() {
            m.mark(&base, "/styles");
        }
        for (key, style) in source {
            let p = path(&base, key);
            let old = style.get("id")?.as_str()?;
            if old.is_empty() || (group == "styles" && old != key) {
                return None;
            }
            let target = if group == "componentDefaults" {
                if !matches!(key.as_str(), "rectangle" | "ellipse") {
                    return None;
                }
                Some(key.as_str())
            } else {
                // A named style has no source component type; preserve its
                // reusable properties for a rectangle only when shape-specific.
                None
            };
            let mut properties = json!({});
            for (k, value) in style.as_object()? {
                if matches!(k.as_str(), "id" | "displayName") {
                    continue;
                }
                if value.is_null() {
                    return None;
                }
                let (dest, val) = style_value(k, value, m)?;
                properties[dest] = val;
            }
            let shape_specific = ["fill", "strokeColor", "strokeWidth", "cornerRadius"]
                .iter()
                .any(|k| properties.get(k).is_some());
            let target = target.or(shape_specific.then_some("rectangle"));
            if target == Some("ellipse") && properties.get("cornerRadius").is_some() {
                return None;
            }
            let t = format!("/styles/{}", m.styles.len());
            let id = m.id(&path(&p, "id"), old, "style");
            let name = match style.get("displayName") {
                Some(x) => m.text(x, &path(&p, "displayName"), &path(&t, "name"))?,
                None => old.to_owned(),
            };
            m.mark(&path(&p, "id"), &path(&t, "id"));
            for (k, value) in style.as_object()? {
                if let Some((dest, _)) = style_value(k, value, m) {
                    m.mark(&path(&p, k), &format!("{t}/properties/{dest}"));
                }
            }
            let mut mapped = json!({"id":id,"name":name,"properties":properties});
            if let Some(target) = target {
                mapped["target"] = json!(target);
            }
            m.styles.push(mapped.clone());
            if group == "styles" {
                m.named_styles.insert(key.clone(), mapped);
            } else {
                m.default_styles.insert(key.clone(), mapped);
            }
        }
    }
    Some(())
}

pub(super) fn inherited(
    source: &Value,
    kind: &str,
    p: &str,
    t: &str,
    m: &mut Mapping,
) -> Option<Value> {
    let mut props = json!({});
    let named = source.get("properties").and_then(|v| v.get("styleId"));
    let defaults = m.default_styles.get(kind);
    if named.is_none() && defaults.is_none() {
        return Some(props);
    }
    if !matches!(kind, "rectangle" | "ellipse") {
        return None;
    }
    if let Some(defaults) = defaults {
        props = defaults["properties"].clone();
    }
    if let Some(named) = named {
        let style = m.named_styles.get(named.as_str()?)?;
        if style.get("target").is_some_and(|target| target != kind) {
            return None;
        }
        for (k, v) in style["properties"].as_object()? {
            props[k] = v.clone();
        }
        let sp = format!("{p}/properties/styleId");
        m.mark(&sp, &format!("{t}/properties"));
        if let Some(f) = m.findings.get_mut(&sp) {
            f.code = "legacy-style-cascade-flattened";
            f.severity = "warning";
            f.message =
                "Named style and defaults resolved at import; reusable declarations retained.";
        }
    }
    Some(props)
}
