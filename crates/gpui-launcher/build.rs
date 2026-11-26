#[path = "build/landscapes.rs"]
mod landscapes;
fn main() {
    let root = std::path::Path::new("../../design-system/landscapes");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    landscapes::generate(root, output.join("landscape_catalog.rs"), None)
        .expect("compile Home landscape catalog");
    landscapes::generate(
        root,
        output.join("landscape_fixture.rs"),
        Some(std::path::Path::new(
            "tests/fixtures/additional-scenic.json",
        )),
    )
    .expect("compile test-only landscape catalog");
    println!("cargo:rerun-if-changed=windows.rc");
    println!("cargo:rerun-if-changed=../../src-tauri/icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("embed the Carbon icon");
    }
}
