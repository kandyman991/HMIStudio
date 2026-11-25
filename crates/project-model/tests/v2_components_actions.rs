//! Original fixtures for candidate component/action contracts.
#![allow(clippy::unwrap_used)]
mod common;
use common::*;
use hmi_project_model::id::IdAllocator;
use hmi_project_model::model::v2::IndustrialSecurityDeclarations;
use hmi_project_model::model::v2_bindings::ExpressionEnumerationDeclarations;
use hmi_project_model::model::v2_components::*;
use hmi_project_model::model::*;
use hmi_project_model::symbols::build_symbol;
use hmi_project_model::validation::validate_v2_components;
use serde_json::{json, Value};

fn declarations() -> Value {
    json!({
        "definitions":[{"definition":"cmp-lamp","classification":{"kind":"faceplate"}}],
        "components":[
            {"id":"new-line","contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":10,"y":10},"strokeColor":"#000000","strokeWidth":1}},
            {"id":"new-arrow","contract":{"kind":"arrow","start":{"x":0,"y":0},"end":{"x":10,"y":0},"strokeColor":"#000000","strokeWidth":2,"headLength":3}},
            {"id":"new-polygon","contract":{"kind":"polygon","points":[{"x":0,"y":0},{"x":10,"y":0},{"x":0,"y":10}],"fill":"#FFFFFF","strokeColor":"#000000","strokeWidth":1}},
            {"id":"new-input","contract":{"kind":"textInput","tag":"tag-text","maxLength":20,"label":{"textKey":"hello"}}},
            {"id":"new-dropdown","contract":{"kind":"dropDown","tag":"tag-word","enumeration":"enum-word"}}
        ],
        "handlers":[{"origin":{"node":"n-text"},"event":"click","actions":[{"type":"navigateStartup"},{"type":"openFaceplate","definition":"cmp-lamp","parameters":{"source":"tag-bool-ro"}},{"type":"back"}]}]
    })
}
fn bindings() -> ExpressionEnumerationDeclarations {
    serde_json::from_value(json!({"enumerations":[{"id":"enum-word","name":"State","dataType":"uint16","entries":[{"value":1,"label":"One"}]}]})).unwrap()
}
fn base_rw() -> Value {
    let mut p = base();
    p["tags"][3]["access"] = json!("readWrite");
    p["tags"][4]["access"] = json!("readWrite");
    p
}
fn check(p: Value, d: Value) -> hmi_project_model::ValidationReport {
    validate_v2_components(
        &project(p),
        &IndustrialSecurityDeclarations::default(),
        &bindings(),
        &serde_json::from_value(d).unwrap(),
    )
}
#[test]
fn candidate_components_actions_roundtrip() {
    let d: ComponentActionDeclarations = serde_json::from_value(declarations()).unwrap();
    assert_eq!(
        d,
        serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap()
    );
    let r = check(base_rw(), declarations());
    assert!(r.is_valid(), "{r}");
    assert!(matches!(
        d.handlers[0].actions[0],
        CandidateAction::New(NewAction::NavigateStartup {})
    ));
    assert!(matches!(
        d.handlers[0].actions[2],
        CandidateAction::Existing(Action::Back {})
    ));
}
#[test]
fn geometry_and_input_rejections() {
    let mut d = declarations();
    d["components"][0]["contract"]["end"] = json!({"x":0,"y":0});
    d["components"][1]["contract"]["headLength"] = json!(11);
    d["components"][2]["contract"]["points"][2] = json!({"x":20,"y":0});
    d["components"][3]["contract"]["maxLength"] = json!(0);
    let r = check(base(), d);
    for path in [
        "/v2/components/components/0/contract/end",
        "/v2/components/components/1/contract/headLength",
        "/v2/components/components/2/contract/points",
        "/v2/components/components/3/contract/tag",
        "/v2/components/components/3/contract/maxLength",
        "/v2/components/components/4/contract/tag",
    ] {
        assert!(r.errors().any(|d| d.path == path), "{path}: {r}");
    }
}
#[test]
fn faceplate_arguments_and_handler_conflicts_are_rejected() {
    let mut d = declarations();
    d["handlers"][0]["origin"] = json!({"node":"n-button"});
    d["handlers"][0]["actions"][1]["parameters"] = json!({"source":"tag-word","other":"x"});
    let r = check(base_rw(), d);
    for path in [
        "/v2/components/handlers/0/event",
        "/v2/components/handlers/0/actions/1/parameters/source",
        "/v2/components/handlers/0/actions/1/parameters/other",
    ] {
        assert!(r.errors().any(|d| d.path == path), "{path}: {r}");
    }
    let mut d = declarations();
    d["definitions"] = json!([]);
    d["handlers"][0]["actions"][1]["parameters"] = json!({});
    let r = check(base_rw(), d);
    assert!(r.error_count() >= 2, "{r}");
}
#[test]
fn action_fields_fail_closed() {
    let mut d = declarations();
    d["handlers"][0]["actions"][0]["screen"] = json!("scr-main");
    assert!(serde_json::from_value::<ComponentActionDeclarations>(d).is_err());
}
#[test]
fn all_nine_symbols_are_valid_unique_passive_definitions() {
    let mut p = project(base());
    let mut alloc = IdAllocator::default();
    let mut d = ComponentActionDeclarations::default();
    for k in IndustrialSymbolKind::ALL {
        let def = build_symbol(k, &mut alloc).unwrap();
        d.definitions.push(DefinitionClassification {
            definition: def.id.clone(),
            classification: DefinitionKind::IndustrialSymbol { symbol: k },
        });
        p.components.push(def);
    }
    let report = validate_v2_components(
        &p,
        &IndustrialSecurityDeclarations::default(),
        &ExpressionEnumerationDeclarations::default(),
        &d,
    );
    assert!(report.is_valid(), "{report}");
    let symbol = p.components.last().unwrap();
    d.handlers.push(CandidateHandler {
        origin: EventOrigin::Node(symbol.root.id.clone()),
        event: EventKind::Click,
        confirm: None,
        actions: vec![CandidateAction::New(NewAction::NavigateStartup {})],
    });
    assert!(validate_v2_components(
        &p,
        &IndustrialSecurityDeclarations::default(),
        &ExpressionEnumerationDeclarations::default(),
        &d
    )
    .has_errors());
}

#[test]
fn startup_action_resolves_current_start_screen_and_never_a_popup() {
    let action = NewAction::NavigateStartup {};
    let mut p = project(base());
    assert_eq!(action.startup_target(&p).unwrap().as_str(), "scr-main");
    let mut second = p.screens[0].clone();
    second.id = ScreenId::new("scr-second").unwrap();
    p.screens.push(second);
    p.navigation.start_screen = ScreenId::new("scr-second").unwrap();
    assert_eq!(action.startup_target(&p).unwrap().as_str(), "scr-second");
    p.navigation.start_screen = ScreenId::new("pop-info").unwrap();
    assert!(action.startup_target(&p).is_none());
}
