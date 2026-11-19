//! 跨平台环境探测：openconnect 可执行文件、TUN/TAP 可用性、提权状态、wintun。
//!
//! 全部为只读检测，不安装驱动、不修改路由表与 DNS。

use crate::model::EnvReport;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 探测 openconnect 可执行文件与运行环境。
///
/// `resource_openconnect` 为打包内置的 openconnect 路径（仅 Windows 使用，
/// 由调用方从 Tauri resource_dir 解析后传入）。
pub fn probe(resource_openconnect: Option<PathBuf>, override_path: Option<&str>) -> EnvReport {
    let platform = std::env::consts::OS.to_string();
    let mut warnings: Vec<String> = Vec::new();

    // 1) 候选路径
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = override_path {
        if !p.trim().is_empty() {
            candidates.push(PathBuf::from(p.trim()));
        }
    }
    if let Some(p) = resource_openconnect {
        candidates.push(p);
    }
    candidates.extend(platform_candidates());
    if let Some(p) = which_openconnect() {
        candidates.push(p);
    }

    // 去重并只保留真实存在的文件
    let mut seen = std::collections::HashSet::new();
    let mut existing: Vec<String> = Vec::new();
    for c in candidates {
        let key = c.to_string_lossy().to_string();
        if !seen.insert(key.clone()) {
            continue;
        }
        if is_executable_file(&c) {
            existing.push(key);
        }
    }

    let openconnect_path = existing.first().cloned();
    let openconnect_version = openconnect_path
        .as_deref()
        .and_then(|p| read_version(Path::new(p)));

    if openconnect_path.is_none() {
        warnings.push(match platform.as_str() {
            "windows" => "未找到 openconnect.exe。请将其放入程序目录 resources/openconnect/ 下，或在配置中手动指定路径。".into(),
            "macos" => "未找到 openconnect。请执行 `brew install openconnect` 后重试，或在配置中手动指定路径。".into(),
            _ => "未找到 openconnect。请通过包管理器安装（如 apt install openconnect），或在配置中手动指定路径。".into(),
        });
    }

    // 2) 提权状态
    //    macOS：GUI 以普通用户运行，openconnect 由 root 辅助进程拉起，
    //    因此这里不把「GUI 未提权」当作问题（否则每次启动都报一条假告警）。
    let mut elevated = is_elevated();
    let elevation_hint = if platform == "macos" {
        elevated = true;
        "openconnect 由 root 辅助进程拉起，GUI 自身以普通用户运行。".to_string()
    } else if elevated {
        "已具备管理员/root 权限。".to_string()
    } else {
        match platform.as_str() {
            "windows" => "当前未以管理员身份运行。建立隧道需要管理员权限：请右键程序图标选择「以管理员身份运行」。".into(),
            _ => format!(
                "当前未以 root 运行。请使用 sudo 启动：\nsudo \"{}\"",
                current_exe_string()
            ),
        }
    };
    if !elevated {
        warnings.push("缺少管理员/root 权限，连接将失败。".into());
    }

    // 3) TUN/TAP 与 wintun
    let (tun_ready, tun_detail) = detect_tun(openconnect_path.as_deref());
    if !tun_ready {
        warnings.push(format!("虚拟网卡不可用：{tun_detail}"));
    }
    let (wintun_ready, wintun_detail) = detect_wintun(&platform, openconnect_path.as_deref());
    if wintun_ready == Some(false) {
        warnings.push("wintun.dll 未就绪，Windows 下 openconnect 无法创建虚拟网卡。".into());
    }

    EnvReport {
        platform,
        openconnect_path,
        openconnect_version,
        candidates: existing,
        elevated,
        elevation_hint,
        tun_ready,
        tun_detail,
        wintun_ready,
        wintun_detail,
        warnings,
    }
}

fn current_exe_string() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "<程序路径>".into())
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

fn platform_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "macos")]
    {
        // Homebrew (Apple Silicon / Intel)、MacPorts、系统路径
        v.push(PathBuf::from("/opt/homebrew/bin/openconnect"));
        v.push(PathBuf::from("/usr/local/bin/openconnect"));
        v.push(PathBuf::from("/opt/local/bin/openconnect"));
        v.push(PathBuf::from("/usr/bin/openconnect"));
        v.push(PathBuf::from("/opt/homebrew/sbin/openconnect"));
    }
    #[cfg(target_os = "linux")]
    {
        v.push(PathBuf::from("/usr/bin/openconnect"));
        v.push(PathBuf::from("/usr/sbin/openconnect"));
        v.push(PathBuf::from("/usr/local/bin/openconnect"));
        v.push(PathBuf::from("/snap/bin/openconnect"));
        v.push(PathBuf::from("/var/lib/flatpak/exports/bin/openconnect"));
    }
    #[cfg(target_os = "windows")]
    {
        v.push(PathBuf::from("C:\\Program Files\\OpenConnect\\openconnect.exe"));
        v.push(PathBuf::from("C:\\Program Files (x86)\\OpenConnect\\openconnect.exe"));
    }
    v
}

/// 在 PATH 中查找 openconnect（不依赖第三方 which crate）。
fn which_openconnect() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "openconnect.exe"
    } else {
        "openconnect"
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// 读取 openconnect 版本号。openconnect 把 --version 输出到 stderr。
fn read_version(bin: &Path) -> Option<String> {
    let mut cmd = Command::new(bin);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr);
    // 形如 "OpenConnect version v9.12"
    text.lines()
        .find(|l| l.to_lowercase().contains("openconnect version"))
        .map(|l| l.trim().to_string())
        .or_else(|| text.lines().next().map(|l| l.trim().to_string()))
}

#[cfg(unix)]
fn is_elevated() -> bool {
    // SAFETY: geteuid 无参数、无副作用、线程安全。
    unsafe { libc::geteuid() == 0 }
}

#[cfg(windows)]
fn is_elevated() -> bool {
    // SAFETY: IsUserAnAdmin 为纯查询，无参数。
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

/// Windows 下 wintun 就绪性；其他平台返回 None。
#[cfg(target_os = "windows")]
fn detect_wintun(_platform: &str, openconnect_path: Option<&str>) -> (Option<bool>, String) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(p) = openconnect_path {
        if let Some(parent) = Path::new(p).parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(sysroot) = std::env::var("SystemRoot") {
        dirs.push(PathBuf::from(sysroot).join("System32"));
    }
    for d in &dirs {
        if d.join("wintun.dll").is_file() {
            return (
                Some(true),
                format!("已检测到 wintun.dll：{}", d.join("wintun.dll").display()),
            );
        }
    }
    (
        Some(false),
        format!(
            "未在以下位置找到 wintun.dll：{}。请先通过安装包部署 wintun 驱动后再连接。",
            dirs.iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join("、")
        ),
    )
}

#[cfg(not(target_os = "windows"))]
fn detect_wintun(_platform: &str, _openconnect_path: Option<&str>) -> (Option<bool>, String) {
    (None, "仅 Windows 平台涉及 wintun。".into())
}

/// TUN/TAP 可用性检测。
#[cfg(target_os = "linux")]
fn detect_tun(_openconnect: Option<&str>) -> (bool, String) {
    let dev = Path::new("/dev/net/tun");
    if !dev.exists() {
        return (
            false,
            "/dev/net/tun 不存在，需加载 tun 模块：sudo modprobe tun".into(),
        );
    }
    let module_loaded = Path::new("/sys/module/tun").exists()
        || std::fs::read_to_string("/proc/modules")
            .map(|s| s.lines().any(|l| l.starts_with("tun ")))
            .unwrap_or(false);
    if module_loaded {
        (true, "/dev/net/tun 可用，tun 模块已加载。".into())
    } else {
        (
            true,
            "/dev/net/tun 存在，但未在 /proc/modules 中确认 tun 模块（可能已内建于内核）。".into(),
        )
    }
}

#[cfg(target_os = "macos")]
fn detect_tun(_openconnect: Option<&str>) -> (bool, String) {
    // macOS 的 utun 由内核按需创建，通过 ifconfig -l 观察是否具备 utun 接口族。
    let out = Command::new("ifconfig")
        .arg("-l")
        .stdin(Stdio::null())
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            if text.split_whitespace().any(|i| i.starts_with("utun")) {
                (true, "utun 网络接口可用。".into())
            } else {
                (
                    false,
                    "未发现 utun 接口。请确认系统网络内核扩展工作正常。".into(),
                )
            }
        }
        Err(e) => (false, format!("无法执行 ifconfig：{e}")),
    }
}

#[cfg(target_os = "windows")]
fn detect_tun(openconnect: Option<&str>) -> (bool, String) {
    // Windows 的虚拟网卡由 wintun 提供，而 wintun.dll 是 side-by-side 部署在
    // openconnect.exe 同目录的，不会出现在 System32（那是驱动注册后的另一回事）。
    // 只查 System32 会在便携版上误报“虚拟网卡不可用”，这里与 detect_wintun 保持一致。
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(p) = openconnect {
        if let Some(parent) = Path::new(p).parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }
    if let Ok(sysroot) = std::env::var("SystemRoot") {
        dirs.push(PathBuf::from(sysroot).join("System32"));
    }
    for d in &dirs {
        if d.join("wintun.dll").is_file() {
            return (true, format!("wintun.dll 就绪：{}", d.join("wintun.dll").display()));
        }
    }
    (
        false,
        "未找到 wintun.dll（详见 wintun 检测结果）。".into(),
    )
}