// 自定义命令通过 app manifest 声明，只有 capability 中列出的窗口才能调用。
const COMMANDS: &[&str] = &[
    "snapshot",
    "detail",
    "submit",
    "dismiss",
    "save_draft",
    "get_prefs",
    "set_prefs",
    "integration",
    "open_external",
    "show_inbox",
    "show_quick",
    "hide_quick",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build");
}
