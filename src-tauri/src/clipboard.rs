//! 系统剪贴板写入（后端实现）。
//!
//! 为什么不用浏览器的 navigator.clipboard：macOS 上本程序经系统密码框以 root
//! 运行，而剪贴板（pasteboard）按用户会话隔离。root 进程写入的是 root 自己的
//! 剪贴板，登录用户的会话里读不到——表现为「复制按钮和 Cmd+C 都没反应」。
//!
//! 解决：以 root 运行时，通过 `launchctl asuser <登录用户 uid>` 切换到登录
//! 用户的会话再执行 pbcopy。注意 pbcopy 依赖 LANG 判断编码，sudo/osascript
//! 提权后环境变量被清空，必须显式指定 UTF-8，否则中文会丢失。

use std::io::Write;
use std::process::{Command, Stdio};

/// 单次复制上限，避免异常情况下向子进程写入过大数据
const MAX_BYTES: usize = 8 * 1024 * 1024;

pub fn write_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err(format!(
            "内容过大（{} MB），请先导出日志文件",
            text.len() / 1024 / 1024
        ));
    }
    let mut cmd = build_command()?;
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动剪贴板程序：{e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("写入剪贴板失败：{e}"))?;
        // drop 关闭管道，通知对方输入结束
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("等待剪贴板程序失败：{e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() {
            format!("剪贴板程序异常退出（{}）", out.status)
        } else {
            format!("剪贴板写入失败：{msg}")
        })
    }
}

#[cfg(target_os = "macos")]
fn build_command() -> Result<Command, String> {
    // SAFETY: geteuid 无参数、无副作用。
    let is_root = unsafe { libc::geteuid() } == 0;
    if !is_root {
        let mut c = Command::new("/usr/bin/pbcopy");
        c.env("LANG", "en_US.UTF-8");
        return Ok(c);
    }
    let uid = console_uid().ok_or("无法确定当前登录用户，复制失败")?;
    let mut c = Command::new("/bin/launchctl");
    c.args([
        "asuser",
        &uid.to_string(),
        "/usr/bin/env",
        "LANG=en_US.UTF-8",
        "/usr/bin/pbcopy",
    ]);
    Ok(c)
}

/// /dev/console 的属主即当前桌面登录用户
#[cfg(target_os = "macos")]
fn console_uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/dev/console").ok()?.uid();
    // uid 0 表示停留在登录窗口，没有可用的用户会话
    (uid != 0).then_some(uid)
}

#[cfg(target_os = "windows")]
fn build_command() -> Result<Command, String> {
    // UAC 提权不切换用户会话，剪贴板与当前用户共享。
    // clip.exe 按 UTF-16LE 解析带 BOM 的输入最可靠，但 UTF-8 在 Win10+ 亦可用；
    // 这里通过 PowerShell 读取 UTF-8 stdin，避免 clip.exe 的代码页乱码问题。
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::InputEncoding=[Text.Encoding]::UTF8; Set-Clipboard -Value ([Console]::In.ReadToEnd())",
    ])
    .creation_flags(CREATE_NO_WINDOW);
    Ok(c)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn build_command() -> Result<Command, String> {
    // Linux：sudo 运行时 DISPLAY / WAYLAND_DISPLAY 往往被清空，优先 wl-copy，其次 xclip
    for (bin, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
        ("xsel", &["--clipboard", "--input"][..]),
    ] {
        if std::path::Path::new("/usr/bin").join(bin).exists() {
            let mut c = Command::new(bin);
            c.args(args);
            return Ok(c);
        }
    }
    Err("未找到剪贴板工具，请安装 wl-clipboard 或 xclip".into())
}

#[tauri::command]
pub fn clipboard_write(text: String) -> Result<(), String> {
    write_text(&text)
}
