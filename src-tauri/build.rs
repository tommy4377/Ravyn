use std::path::Path;

/// Generated only for `--features mcp-automation` builds so the debug-only
/// automation bridge permission never ships in default builds (where the
/// plugin is not compiled and the permission would fail manifest validation).
const MCP_CAPABILITY_PATH: &str = "capabilities/mcp-automation.gen.json";

const MCP_CAPABILITY: &str = r#"{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "mcp-automation-capability",
  "description": "Generated debug-automation grant; never committed or shipped.",
  "windows": ["main", "setup", "compact"],
  "permissions": ["mcp-bridge:default"]
}
"#;

fn sync_mcp_capability() {
    let enabled = std::env::var_os("CARGO_FEATURE_MCP_AUTOMATION").is_some();
    let path = Path::new(MCP_CAPABILITY_PATH);
    if enabled {
        std::fs::write(path, MCP_CAPABILITY)
            .expect("failed to write the generated mcp-automation capability");
    } else if path.exists() {
        std::fs::remove_file(path).expect("failed to remove the stale mcp-automation capability");
    }
}

fn main() {
    sync_mcp_capability();
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
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
            "prompt_torrent_default_app",
            "notify_native",
            "open_compact_window",
            "focus_main_window",
        ]),
    ))
    .expect("failed to build the Ravyn Tauri application manifest");
}
