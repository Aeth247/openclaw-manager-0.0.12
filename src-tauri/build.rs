fn main() {
    // Windows：嵌入清单，启动时请求管理员权限（便于停止/管理已提权启动的网关进程）
    let mut windows = tauri_build::WindowsAttributes::new();
    windows = windows.app_manifest(include_str!("windows/app.manifest"));
    let attrs = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
