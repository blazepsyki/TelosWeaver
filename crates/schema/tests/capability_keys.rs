//! Fixed key spellings and open-map wire compatibility (Spec §7.1, REQ-024).

use serde_json::json;
use std::collections::{HashMap, HashSet};
use telos_schema::{Capability, capability_keys};

#[test]
fn common_names_are_the_eight_spec_strings() {
    // These literals pin the external contract independently of the constants.
    assert_eq!(
        capability_keys::ALL,
        &[
            "managed_run",
            "event_stream",
            "cancel",
            "native_resume",
            "native_fork",
            "send_input",
            "answer_approval",
            "vscode_listing",
        ]
    );
    assert_eq!(capability_keys::ALL.iter().collect::<HashSet<_>>().len(), 8);
}

#[test]
fn generated_key_catalogue_uses_the_same_known_names() {
    let (_, schema) = telos_schema::json_schemas()
        .into_iter()
        .find(|(name, _)| *name == "CommonCapabilityKey")
        .unwrap();
    let value = serde_json::to_value(schema).unwrap();
    assert_eq!(value["title"], "CommonCapabilityKey");
    assert_eq!(value["type"], "string");
    assert_eq!(value["enum"], json!(capability_keys::ALL));
}

#[test]
fn string_maps_preserve_common_provider_and_unknown_keys_and_capability_values() {
    let value = json!({
        "state": "unverified", "providerVersion": "fixture-version", "hostId": "fixture-host",
        "checkedAt": "2026-10-26T00:00:00Z", "evidenceLevel": "contract-tested",
    });
    let capability: Capability = serde_json::from_value(value.clone()).unwrap();
    let keys = [
        capability_keys::MANAGED_RUN,
        "provider.codex.future_feature",
        "opaque_future_key",
    ];
    let map: HashMap<String, Capability> = keys
        .iter()
        .map(|key| (key.to_string(), capability.clone()))
        .collect();
    let encoded = serde_json::to_value(&map).unwrap();
    assert_eq!(encoded.as_object().unwrap().len(), 3);
    for key in keys {
        assert_eq!(encoded[key], value);
    }
    let decoded: HashMap<String, Capability> = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, map);
}
