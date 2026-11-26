// SPDX-License-Identifier: MPL-2.0
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: u16,
    views: Vec<View>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct View {
    id: String,
    family: String,
    name: String,
    attribution: String,
    poster: String,
    frames: Option<String>,
    fps: Option<u8>,
    bake: Option<Bake>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bake {
    source: String,
    camera: String,
}

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn asset(root: &Path, name: &str) -> Result<String, String> {
    let path = Path::new(name);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(format!("Expected a local bundled asset name, got {name}"));
    }
    let full = root.join(path);
    if !fs::symlink_metadata(&full).is_ok_and(|meta| meta.is_file()) {
        return Err(format!("Missing regular bundled asset {name}"));
    }
    println!("cargo:rerun-if-changed={}", full.display());
    Ok(format!(
        "include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../design-system/landscapes/\", {name:?}))"
    ))
}

pub fn generate(root: &Path, output: PathBuf, fixture: Option<&Path>) -> Result<(), String> {
    let manifest = root.join("manifest.json");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let mut catalog: Catalog =
        serde_json::from_slice(&fs::read(&manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if catalog.version != 1 || catalog.views.is_empty() {
        return Err("Expected a populated version 1 landscape catalog".into());
    }
    if let Some(fixture) = fixture {
        println!("cargo:rerun-if-changed={}", fixture.display());
        let entry = serde_json::from_slice(&fs::read(fixture).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        catalog.views.push(entry);
    }
    let sources_file = root.join("../../tools/landscapes/sources.json");
    println!("cargo:rerun-if-changed={}", sources_file.display());
    let sources: serde_json::Value =
        serde_json::from_slice(&fs::read(&sources_file).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut ids = BTreeSet::new();
    let mut entries = Vec::new();
    for view in &catalog.views {
        if !identifier(&view.id)
            || !identifier(&view.family)
            || !ids.insert(&view.id)
            || view.name.trim().is_empty()
            || view.attribution.trim().is_empty()
        {
            return Err(format!(
                "Invalid identity, title or attribution for {}",
                view.id
            ));
        }
        if let Some(bake) = &view.bake
            && (!identifier(&bake.source)
                || !identifier(&bake.camera)
                || !sources[&bake.source]["cameras"][&bake.camera].is_object())
        {
            return Err(format!(
                "Unknown authoring source or camera for {}",
                view.id
            ));
        }
        let poster = asset(root, &view.poster)?;
        let (motion, rate) = match (&view.frames, view.fps) {
            (Some(file), Some(fps @ 1..=30)) => (format!("Some({})", asset(root, file)?), fps),
            (None, None) => ("None".to_owned(), 0),
            _ => {
                return Err(format!(
                    "Expected motion frames with 1-30 fps, or a still poster, for {}",
                    view.id
                ));
            }
        };
        entries.push(format!("Landscape(&Scene {{ id:{:?}, family:{:?}, name:{:?}, attribution:{:?}, poster:{poster}, motion:{motion}, fps:{rate} }})",view.id,view.family,view.name,view.attribution));
    }
    fs::write(output, format!("[{}]", entries.join(",\n"))).map_err(|e| e.to_string())
}
