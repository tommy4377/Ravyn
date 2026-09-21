fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "backend_info",
                "setup_installation_info",
                "apply_windows_integration",
                "finish_setup_handoff",
                "restart_application",
                "main_window_ready",
                "app_update_status",
                "check_app_update",
                "desktop_appearance",
                "open_native_path",
                "reveal_native_path",
            ]),
        ),
    )
    .expect("failed to build the Ravyn Tauri application manifest");
}
