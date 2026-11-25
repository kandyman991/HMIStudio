//! Synthetic cross-runtime vectors; expected observations come from the
//! pinned private contract-1 oracle, not from a second Rust implementation.
use hmi_project_model::expression::{compile, Quality, Reading, Type, Value};
use std::collections::BTreeMap;
pub fn observe(expressions: &[String]) -> Vec<String> {
    let mut observed = Vec::with_capacity(expressions.len());
    let types: BTreeMap<String, Type> = [
        "good",
        "uncertain",
        "bad",
        "missing",
        "empty",
        "nonfinite",
        "good-id",
    ]
    .into_iter()
    .map(|x| (x.into(), Type::Number))
    .chain([("true".into(), Type::Bool), ("😀".into(), Type::Text)])
    .collect();
    let reading = |v, q| Reading {
        value: v,
        quality: q,
    };
    let readings = BTreeMap::from([
        (
            "good".into(),
            reading(Some(Value::Number(3.0)), Quality::Good),
        ),
        (
            "uncertain".into(),
            reading(Some(Value::Number(2.0)), Quality::Uncertain),
        ),
        (
            "bad".into(),
            reading(Some(Value::Number(99.0)), Quality::Bad),
        ),
        ("empty".into(), reading(None, Quality::Good)),
        (
            "nonfinite".into(),
            reading(Some(Value::Number(f64::INFINITY)), Quality::Good),
        ),
        (
            "good-id".into(),
            reading(Some(Value::Number(4.0)), Quality::Good),
        ),
        (
            "true".into(),
            reading(Some(Value::Bool(true)), Quality::Good),
        ),
        (
            "😀".into(),
            reading(Some(Value::Text("ok".into())), Quality::Good),
        ),
    ]);
    for (i, s) in expressions.iter().enumerate() {
        let Ok(e) = compile(s, &types) else {
            observed.push(format!("{i}\tREJECT"));
            continue;
        };
        let ty = match e.result_type() {
            Type::Bool => "BOOL",
            Type::Number => "NUMBER",
            Type::Text => "TEXT",
        };
        let refs = e.references().join(",");
        let x = e.evaluate(&readings);
        let q = match x.quality {
            Quality::Good => "GOOD",
            Quality::Uncertain => "UNCERTAIN",
            Quality::Bad => "BAD",
        };
        let val = match x.value {
            Some(Value::Bool(b)) => format!("B:{b}"),
            Some(Value::Number(n)) => format!("N:{:016x}", n.to_bits()),
            Some(Value::Text(s)) => format!(
                "T:{}",
                s.as_bytes()
                    .iter()
                    .map(|x| format!("{x:02x}"))
                    .collect::<String>()
            ),
            None => "-".into(),
        };
        observed.push(format!(
            "{i}\tOK\t{ty}\t{refs}\t{q}\t{val}\t{}",
            x.error.is_some()
        ));
    }
    observed
}
