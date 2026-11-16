//! Tauri 命令层：前端唯一的后端入口。
//!
//! 所有命令都做前置校验并返回可直接展示给用户的中文错误信息，
//! 便于前端统一以弹窗形式呈现。

use crate::model::{EnvReport, LogLine, Profile, StateSnapshot};
use crate::probe;
use crate::runner::Runner;
use crate::store::Store;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager, State};

/// 随包内置的 openconnect 相对路径（相对 resource 目录）。
/// 找不到时返回 None，由 probe 回退到系统安装的版本。
#[cfg(target_os = "windows")]
const BUNDLED_OC: Option<&str> = Some("openconnect/openconnect.exe");

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const BUNDLED_OC: Option<&str> = Some("openconnect/macos-arm64/openconnect");

// Intel Mac 暂不提供内置运行时，回退到系统安装的 openconnect
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const BUNDLED_OC: Option<&str> = None;

#[cfg(all(unix, not(target_os = "macos")))]
const BUNDLED_OC: Option<&str> = None;

pub struct AppState {
    pub store: Store,
    pub runner: Arc<Runner>,
    /// 全局兜底路径（用户在设置中指定），持久化在 prefs.json
    pub prefs: crate::prefs::Prefs,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub license: String,
    pub openconnect_license: String,
    pub copyright: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Octauri".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        platform: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        license: "GPL-3.0-or-later".into(),
        openconnect_license: "OpenConnect 由 OpenConnect 项目开发，以 LGPL-2.1 许可证发布（部分组件为 GPL/LGPL 例外条款）。完整许可证文本见 https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html".into(),
        copyright: format!(
            "本程序是 openconnect 命令行的图形外壳，通过子进程调用 openconnect 完成全部隧道功能，自身不含任何 VPN 协议栈实现。\n\nOpenConnect: Copyright © 2009-2025 OpenConnect Contributors.\nOctauri: Copyright © 2025 Octauri Contributors.\n\n本程序为自由软件，依据 GNU 通用公共许可证第 3 版或更新版本（GPL-3.0-or-later）发布，\n不提供任何担保。任何修改版或再分发均须以同一许可证开放源代码。\n\nOctauri 的名称与图标不在 GPL 授权范围内（GPL 不授予商标权）。"
        ),
    }
}

// ------------------------------ 配置 ------------------------------

#[tauri::command]
pub fn profiles_list(state: State<'_, AppState>) -> Vec<Profile> {
    state.store.load()
}

#[tauri::command]
pub fn profile_save(state: State<'_, AppState>, profile: Profile) -> Result<Profile, String> {
    state.store.upsert(profile)
}

#[tauri::command]
pub fn profile_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // 若该配置正在使用中，先阻止删除，避免断开逻辑失去上下文
    if let Some(pid) = state.runner.running_pid() {
        let snap = state.runner.snapshot();
        if snap.profile_id.as_deref() == Some(id.as_str()) {
            return Err(format!(
                "该配置正在连接中（pid {pid}），请先断开再删除。"
            ));
        }
    }
    state.store.delete(&id)
}

#[tauri::command]
pub fn profile_import(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败：{e}"))?;
    state.store.import(&raw)
}

#[tauri::command]
pub fn profile_export(
    state: State<'_, AppState>,
    path: String,
    ids: Vec<String>,
) -> Result<(), String> {
    let json = state.store.export(&ids)?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, json).map_err(|e| format!("写入文件失败：{e}"))
}

// ------------------------------ 凭据 ------------------------------

// ------------------------------ 环境 ------------------------------

#[tauri::command]
pub fn env_probe(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> EnvReport {
    let override_path = profile_id
        .as_ref()
        .and_then(|id| state.store.load().into_iter().find(|p| &p.id == id))
        .map(|p| p.openconnect_path)
        .filter(|p| !p.is_empty())
        .or_else(|| state.prefs.get().openconnect_path.clone());

    probe::probe(resource_openconnect(&app), override_path.as_deref())
}

fn resource_openconnect(app: &AppHandle) -> Option<PathBuf> {
    // Windows 与 macOS 随包内置；Linux 依赖发行版包管理器，不内置。
    let rel = BUNDLED_OC?;
    let p = app.path().resolve(rel, BaseDirectory::Resource).ok()?;
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

// ------------------------------ 连接 ------------------------------

#[tauri::command]
pub fn connect(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    password: Option<String>,
) -> Result<u32, String> {
    let profile = state
        .store
        .load()
        .into_iter()
        .find(|p| p.id == profile_id)
        .ok_or_else(|| "配置不存在，可能已被删除".to_string())?;

    // 1) 解析可执行文件
    let report = probe::probe(
        resource_openconnect(&app),
        Some(profile.openconnect_path.as_str()).filter(|s| !s.is_empty()),
    );
    let bin = match report.openconnect_path.clone() {
        Some(p) => PathBuf::from(p),
        None => {
            return Err(
                "未找到 openconnect 可执行文件。\n请在「环境」面板中指定路径，或先安装 openconnect。"
                    .into(),
            )
        }
    };

    // 2) 权限校验：真实 openconnect 需要 root / 管理员权限。
    //    例外：scripts/mock-openconnect.sh 这类联调替身不碰网卡，无需提权。
    let is_mock = bin
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase().contains("mock"))
        .unwrap_or(false);
    // macOS：GUI 以普通用户运行，openconnect 由 root 辅助进程拉起，
    // 因此不要求 GUI 自身已提权（这是修复「退出后进程残留」的架构前提）。
    #[cfg(target_os = "macos")]
    let raised_by_helper = true;
    #[cfg(not(target_os = "macos"))]
    let raised_by_helper = false;

    if is_mock {
        state
            .runner
            .log(&app, "app", "检测到 mock 可执行文件，已跳过提权校验（仅用于本地联调）");
    } else if !raised_by_helper && !report.elevated {
        return Err(format!(
            "权限不足，无法建立隧道。\n{}",
            report.elevation_hint
        ));
    }
    if report.wintun_ready == Some(false) {
        return Err(format!(
            "wintun 驱动未就绪，无法创建虚拟网卡。\n{}",
            report.wintun_detail
        ));
    }

    // 3) 凭据：不持久化，必须由本次连接传入
    let pass = match profile.auth_method {
        crate::model::AuthMethod::Password => match password.clone().filter(|s| !s.is_empty()) {
            Some(v) => Some(v),
            None => return Err("请输入登录密码".into()),
        },
        // 证书模式下 password 作为私钥口令，可为空
        crate::model::AuthMethod::Certificate => password.clone().filter(|s| !s.is_empty()),
    };

    state.runner.start(&app, &profile, &bin, pass)
}

#[tauri::command]
pub fn disconnect(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.runner.stop(&app, false)
}

#[tauri::command]
pub fn force_kill(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.runner.running_pid().is_none() {
        // 没有进程时也要清理可能残留的 pid 状态
        return Err("当前没有运行中的 openconnect 进程".into());
    }
    state.runner.stop(&app, true)
}

/// root 辅助进程（macOS）是否就绪。GUI 以普通用户运行时，
/// 连接能力依赖它；未就绪且用户已取消授权时，前端给出明确提示。
#[tauri::command]
pub fn helper_status(state: State<'_, AppState>) -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        let ready = state.runner.has_helper();
        serde_json::json!({
            "state": if ready { "ready" } else { "starting" },
            "error": state.runner.helper_error().unwrap_or_default(),
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = &state;
        serde_json::json!({ "state": "ready", "error": "" })
    }
}

#[tauri::command]
pub fn conn_state(state: State<'_, AppState>) -> StateSnapshot {
    state.runner.snapshot()
}

#[tauri::command]
pub fn logs_tail(state: State<'_, AppState>, limit: Option<usize>) -> Vec<LogLine> {
    state.runner.tail(limit.unwrap_or(500))
}

#[tauri::command]
pub fn logs_clear(state: State<'_, AppState>) {
    state.runner.clear_logs();
}

#[tauri::command]
pub fn logs_export(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let lines = state.runner.tail(100_000);
    let text = lines
        .iter()
        .map(|l| format!("[{}] [{}] {}", iso_utc(l.ts), l.stream, l.text))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, text).map_err(|e| format!("写入日志文件失败：{e}"))
}

/// 毫秒时间戳 → ISO-8601 UTC 字符串。
///
/// 不引入 chrono：日志导出面向跨时区排障，统一用 UTC 避免歧义
/// （界面内展示的仍是用户本地时间）。
fn iso_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Howard Hinnant 的 civil_from_days 算法
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::iso_utc;
    use crate::runner::split_args;

    #[test]
    fn iso_utc_matches_known_instants() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_700_000_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_utc(1_735_689_600_000), "2025-01-01T00:00:00Z");
        assert_eq!(iso_utc(1_758_000_000_000), "2025-09-16T05:20:00Z");
        // 闰年 2 月 29 日
        assert_eq!(iso_utc(1_709_164_800_000), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn split_args_handles_quotes_and_spaces() {
        assert_eq!(
            split_args("--no-dtls --verbose"),
            vec!["--no-dtls", "--verbose"]
        );
        assert_eq!(
            split_args(r#"--servercert "pin-sha256:abc" --user=a"#),
            vec!["--servercert", "pin-sha256:abc", "--user=a"]
        );
        assert_eq!(split_args("   "), Vec::<String>::new());
        assert_eq!(
            split_args("--os='Windows 11'"),
            vec!["--os=Windows 11"]
        );
        assert_eq!(split_args("--csd-wrapper ''"), vec!["--csd-wrapper", ""]);
    }
}