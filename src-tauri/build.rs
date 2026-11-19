fn main() {
    // 清单文件被 include_str! 读取；显式声明依赖，改动后才会重新嵌入，
    // 否则修改 requestedExecutionLevel 不会触发重编（cargo 看不到 include_str 依赖）。
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    #[allow(unused_mut)]
    let mut attrs = tauri_build::Attributes::new();

    // Windows：嵌入 requireAdministrator 清单，双击即请求管理员权限
    // （openconnect 创建虚拟网卡必须提权）。
    #[cfg(target_os = "windows")]
    {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new()
                .app_manifest(include_str!("windows-app-manifest.xml")),
        );
    }

    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}