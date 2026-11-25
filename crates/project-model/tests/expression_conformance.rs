//! Reproduce the measured JVM contract-1 outputs without private sources.
#![allow(clippy::unwrap_used)]
#[path = "conformance/rust_observe.rs"]
mod runner;
#[test]
fn pinned_kotlin_contract_1_synthetic_vectors() {
    let vectors: Vec<String> =
        serde_json::from_str(include_str!("conformance/vectors.json")).unwrap();
    let expected: Vec<_> = include_str!("conformance/expected.tsv").lines().collect();
    let actual = runner::observe(&vectors);
    assert_eq!(vectors.len(), 12_664);
    assert_eq!(actual.len(), expected.len());
    for (i, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "vector {i}: {:?}", vectors[i]);
    }
}
