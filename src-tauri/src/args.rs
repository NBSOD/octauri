//! 命令行参数（主要用于调试与自动化验证）。
//!
//! macOS 上通过 `open -n Octauri.app --args ...` 启动时环境变量不方便传，
//! 因此这些开关同时支持命令行形式。正常双击启动不会带任何参数，行为不变。

use std::sync::OnceLock;

#[derive(Debug, Default, Clone)]
pub struct Args {
    /// 跳过 macOS 自动提权（无法建立真实隧道，配合 mock 使用）
    pub no_elevate: bool,
    /// 覆盖单实例端口
    pub port: Option<u16>,
    /// 用指定可执行文件起一个连接（仅 debug 构建生效）
    pub spawn: Option<String>,
    /// N 毫秒后自动退出（仅 debug 构建生效）
    pub exit_after_ms: Option<u64>,
    /// N 毫秒后调用 [NSApp terminate:]（仅 debug 构建生效），验证接管是否生效
    pub terminate_after_ms: Option<u64>,
    /// 以 root 辅助进程模式运行，连接回指定的 GUI 端口（macOS）
    pub helper_port: Option<u16>,
    /// 辅助进程的一次性握手 token 文件（读完即删）
    pub helper_token_file: Option<String>,
}

static ARGS: OnceLock<Args> = OnceLock::new();

pub fn get() -> &'static Args {
    ARGS.get_or_init(parse)
}

fn parse() -> Args {
    let mut a = Args::default();
    let mut helper_mode = false;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        let mut take = || inline.clone().or_else(|| it.next());
        match key.as_str() {
            "--no-elevate" => a.no_elevate = true,
            "--port" => a.port = take().and_then(|v| v.parse().ok()).filter(|p| *p != 0),
            "--spawn" => a.spawn = take(),
            "--exit-after-ms" => a.exit_after_ms = take().and_then(|v| v.parse().ok()),
            "--terminate-after-ms" => a.terminate_after_ms = take().and_then(|v| v.parse().ok()),
            "--helper" => helper_mode = true,
            "--connect-port" => a.helper_port = take().and_then(|v| v.parse().ok()),
            "--helper-token-file" => a.helper_token_file = take(),
            _ => {}
        }
    }
    // --helper 单独出现时缺端口，视作未启用
    a.helper_port = a.helper_port.filter(|_| helper_mode);
    a
}

/// 辅助进程模式（仅 macOS 由 GUI 以 root 启动时使用）
pub fn helper_port() -> Option<u16> {
    get().helper_port
}

pub fn helper_token_file() -> Option<String> {
    get().helper_token_file.clone()
}

/// 环境变量优先于命令行（便于脚本临时覆盖）
pub fn no_elevate() -> bool {
    get().no_elevate || std::env::var_os("OCTAURI_NO_ELEVATE").is_some()
}

pub fn port() -> Option<u16> {
    get()
        .port
        .or_else(|| std::env::var("OCTAURI_PORT").ok().and_then(|v| v.parse().ok()))
        .filter(|p| *p != 0)
}

pub fn spawn() -> Option<String> {
    get()
        .spawn
        .clone()
        .or_else(|| std::env::var("OCTAURI_SELFTEST_SPAWN").ok())
}

pub fn terminate_after_ms() -> Option<u64> {
    get().terminate_after_ms
}

pub fn exit_after_ms() -> Option<u64> {
    get()
        .exit_after_ms
        .or_else(|| std::env::var("OCTAURI_EXIT_AFTER_MS").ok().and_then(|v| v.parse().ok()))
}