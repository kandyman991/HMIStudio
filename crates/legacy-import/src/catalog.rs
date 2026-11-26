//! SHA-256 signatures of normalized standard declaration data from the
//! pinned legacy catalog (92a612d). No legacy implementation is embedded.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SIGNATURES: &[(&str, &str)] = &[
    ("standard.motor", "6aa0bead5f0f3255193dbbae6088c5a9cc2023b6763b847c1dfd3e15233614c2"),
    ("standard.valve", "f5c0f855e4991e1ae532c0fb38550d977d67d3707f02b69f335d180c0cc61c7f"),
    ("standard.conveyor", "cb7db92a3bcc3f6b34a601fd3dd0c5540bc303edbbfa71d7a4b2f39546d89a4a"),
    ("standard.sensor", "ab3134044e473b9386c31c905864fc1a75989faa74414dbd2ec0166a46bbd6f0"),
    ("standard.numericField", "4e1f9a0f290695dbbdf053b078815b11c6d58a2870197241b8fa5215083c85d3"),
    ("standard.button", "c1859f5372a872f3d634ced9f15fee9057f1ec24fe2549c4177f6ded8369cf4d"),
    ("standard.switch", "9cab722b50b17758d44e4f67cfbb9c281f76fd63c93051eab9bdaed61b134943"),
    ("standard.slider", "4f5c506d60cb690180047143753d0704854ddd114d3ccaa12242d7bc0769d5ab"),
    ("standard.numericInput", "11a26b471215d3a2963a3f29fa9b505d9222d6a39d3a8805a321289d965e1574"),
    ("standard.textInput", "556503c824c4994395ae51e9a15b48c89c8ff83c966394b68bc09dbca9f5a022"),
    ("standard.dropDown", "e997c3d2305d3254acd84fa8ff9833204ab1b89c4c7e5e8844fa6ef8e4cb8bf0"),
    ("standard.label", "53fd83a3337a4b2e9fffaab7ef78980bcf7d56187790fb674da7f84cbec017f6"),
    ("standard.indicator", "53aad7a07a05729b3029ceb48d96b5f5d66f12f14853ba61dd7a3397923a507a"),
    ("standard.pump", "466b345c03a31f6e07f66028dbd569a5202c00934508d7cbdb91f5250e2021b7"),
    ("standard.tank", "c5e3962b6a3430faa5a02144e51665222aa27d6cd564f30f059a60eda6a89a28"),
    ("standard.pipe", "36baa351cd634f55b77e4bc3fb3eddeef37d9abe49ff04fa8eb225b6a87c42f4"),
    ("standard.gauge", "7fb8b5ae8e8b3a8930c0987480898d2dee24500508b11fc474ba54413045769c"),
    ("standard.levelIndicator", "19af2e68895ea27d9c19495f6255f61be8d97adafdecd9e81d581b5956125dd4"),
    ("standard.thermalIndicator", "1f02dd37f1caaefb86e7d1780163bcd4c6208bdaf635cb804b2a5e591a454e8e"),
    ("standard.flowIndicator", "c33536d26f2cd7dcd62b7aeb13fd6b0125d523513d5208386caf6c77a1c72d82"),
    ("standard.rectangle", "6f04fa26997e6d094892cce429d9d6c8267b1dc18265e863b4ec8a4579b4be68"),
    ("standard.ellipse", "2c4fd9ed5155b92e37922f8e57f8aacfe8557fcd3b7de9212e86cf95fca5ac9c"),
    ("standard.line", "53a083a2ae9102d5c83a91b85c19ce3f6e502aeca451aa1333d6559d21844881"),
    ("standard.arrow", "e17b5afd61018c5654255e8d3a53ad9f1b7881f3877fa9218d1923657495811c"),
    ("standard.polygon", "e05e7b95684daea44b385ff7af0bc26fdfcda65f2e8672dcb38090df650143ba"),
    ("standard.image", "2d8f435c88a42050d5c47fce2044055d4072d7116feaf0d22fd7793879b2b50d"),
    ("standard.svgImage", "8acf8c6703264bb3e06646992d096d187767089af60a7cf365a31802acf2b499"),
];

pub(super) fn recognize(key: &str, source: &Value) -> Option<()> {
    let (_, expected) = SIGNATURES.iter().find(|(id, _)| *id == key)?;
    let obj = source.as_object()?;
    if obj.keys().any(|k| {
        !matches!(
            k.as_str(),
            "id" | "displayName" | "type" | "bindingSlots" | "defaultProperties"
        )
    }) {
        return None;
    }
    let mut normalized = source.clone();
    for name in ["bindingSlots", "defaultProperties"] {
        if source.get(name).is_none() {
            normalized[name] = json!({});
        }
    }
    normalized["defaultProperties"].as_object()?;
    for slot in normalized["bindingSlots"].as_object_mut()?.values_mut() {
        if slot.as_object()?.keys().any(|k| {
            !matches!(k.as_str(), "role" | "acceptedTypes" | "required" | "writable")
        }) {
            return None;
        }
        for (key, value) in [
            ("acceptedTypes", json!([])),
            ("required", json!(false)),
            ("writable", json!(false)),
        ] {
            if slot.get(key).is_none() {
                slot[key] = value;
            }
        }
        let types = slot["acceptedTypes"].as_array_mut()?;
        if !types.iter().all(Value::is_string) {
            return None;
        }
        types.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    }
    let bytes = serde_json::to_vec(&normalized).ok()?;
    (format!("{:x}", Sha256::digest(bytes)) == *expected).then_some(())
}
