// SPDX-License-Identifier: MPL-2.0
#[path = "../build/landscapes.rs"]
mod catalog;
use serde_json::{Value, json};
use std::fs;

#[test]
fn authoring_references_are_explicit_and_finished_assets_need_no_source() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("design-system/landscapes");
    let tools = temp.path().join("tools/landscapes");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&tools).unwrap();
    fs::write(
        tools.join("sources.json"),
        r#"{"farm":{"cameras":{"above":{}}}}"#,
    )
    .unwrap();
    fs::write(root.join("poster.jpg"), b"fixture").unwrap();
    let build = |spec: Option<Value>| {
        let mut entry = json!({"id":"unrelated-name","family":"valley","name":"Valley","attribution":"Carbon","poster":"poster.jpg"});
        if let Some(source) = spec {
            entry["bake"] = source;
        }
        fs::write(
            root.join("manifest.json"),
            json!({"version":1,"views":[entry]}).to_string(),
        )
        .unwrap();
        catalog::generate(&root, temp.path().join("generated.rs"), None)
    };
    build(None).unwrap();
    build(Some(json!({"source":"farm","camera":"above"}))).unwrap();
    for (source, camera) in [("missing", "above"), ("farm", "absent")] {
        assert!(
            build(Some(json!({"source":source,"camera":camera})))
                .unwrap_err()
                .contains("Unknown authoring source or camera")
        );
    }
}
