fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "edit_profile_dialog",
        "choose_instance_icon",
        "apply_desktop_theme",
        "personal_library",
        "open_game_folder",
        "fixture_library",
        "fixture_action",
        "play_readiness",
        "minecraft_account_status",
        "show_minecraft_accounts",
        "import_pack",
        "inspect_vanilla_profile",
        "import_vanilla_client",
        "select_profile_java",
        "install_profile_java",
        "cancel_java_install",
        "plan_game_files",
        "prepare_game_files",
        "cancel_game_files",
        "run_sandbox_session",
    ]);
    let windows = tauri_build::WindowsAttributes::new()
        .append_rc_content(include_str!("src/dialogs/profile.rc"));
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(manifest)
            .windows_attributes(windows),
    )
    .expect("Build the scoped Carbon Desktop command manifest");
}
