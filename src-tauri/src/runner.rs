//! openconnect 子进程生命周期管理。
//!
//! 设计要点：
//! 1. 本模块只负责「拉起进程 / 喂参数 / 收集日志 / 终止进程」，
//!    不实现任何 VPN 协议逻辑，隧道建立完全由 openconnect 完成。
//! 2. 日志采用「后端环形缓冲 + 事件推送」双通道：前端可增量渲染，
//!    重连窗口或刷新后可通过 tail() 补齐历史，缓冲上限固定 → 不会内存泄漏。
//! 3. 子进程 pid 用 AtomicU32 记录，wait 由独立线程回收，避免僵尸进程。

use crate::model::{now_ms, ConnState, LogLine, StateSnapshot, LOG_BUFFER_CAP};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

pub const EVT_LOG: &str = "oc://log";
pub const EVT_STATE: &str = "oc://state";
/// 服务器证书未通过校验时推给前端，由用户决定是否固定该证书
pub const EVT_CERT: &str = "oc://cert-prompt";

pub struct Runner {
    state: Mutex<StateSnapshot>,
    logs: Mutex<VecDeque<LogLine>>,
    seq: AtomicU64,
    /// 每次启动递增；用于丢弃上一代进程的迟到输出
    generation: AtomicU64,
    pid: AtomicU32,
    /// 标记本次终止是否由用户主动发起（决定最终状态是 idle 还是 error）
    user_stop: AtomicBool,
    /// 串行化启动流程，避免并发点击导致拉起两个进程
    start_lock: Mutex<()>,
    /// 本次连接是否已就证书问题询问过用户（每连接只问一次）
    cert_prompted: AtomicBool,
    /// 暂存最近一次解析到的证书失败原因
    cert_reason: Mutex<String>,
}

impl Runner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(StateSnapshot::default()),
            logs: Mutex::new(VecDeque::with_capacity(LOG_BUFFER_CAP.min(512))),
            seq: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            pid: AtomicU32::new(0),
            user_stop: AtomicBool::new(false),
            start_lock: Mutex::new(()),
            cert_prompted: AtomicBool::new(false),
            cert_reason: Mutex::new(String::new()),
        })
    }

    fn lock_state(&self) -> MutexGuard<'_, StateSnapshot> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn lock_logs(&self) -> MutexGuard<'_, VecDeque<LogLine>> {
        self.logs.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn snapshot(&self) -> StateSnapshot {
        self.lock_state().clone()
    }

    pub fn tail(&self, limit: usize) -> Vec<LogLine> {
        let logs = self.lock_logs();
        let skip = logs.len().saturating_sub(limit);
        logs.iter().skip(skip).cloned().collect()
    }

    pub fn clear_logs(&self) {
        self.lock_logs().clear();
    }

    pub fn running_pid(&self) -> Option<u32> {
        let pid = self.pid.load(Ordering::SeqCst);
        if pid == 0 {
            None
        } else {
            Some(pid)
        }
    }

    /// 追加一条日志并推送前端。
    pub fn log(&self, app: &AppHandle, stream: &str, text: &str) {
        let (level, tag) = classify(text);
        let line = LogLine {
            seq: self.seq.fetch_add(1, Ordering::Relaxed) + 1,
            ts: now_ms(),
            stream: stream.to_string(),
            level: level.to_string(),
            text: text.to_string(),
            tag: tag.map(|t| t.to_string()),
        };
        {
            let mut logs = self.lock_logs();
            if logs.len() >= LOG_BUFFER_CAP {
                // 丢弃最旧的一条，容量恒定
                logs.pop_front();
            }
            logs.push_back(line.clone());
        }
        let _ = app.emit(EVT_LOG, line);
    }

    fn transition(&self, app: &AppHandle, state: ConnState, message: &str) {
        let snap = {
            let mut s = self.lock_state();
            let pid = self.running_pid();
            if s.state == state.as_str() && s.message == message && s.pid == pid {
                return;
            }
            if matches!(state, ConnState::Idle | ConnState::Error) {
                s.tunnel_ip = None;
            }
            s.state = state.as_str().to_string();
            s.message = message.to_string();
            s.since = now_ms();
            s.pid = pid;
            s.clone()
        };
        let _ = app.emit(EVT_STATE, snap);
    }

    /// 从一行 openconnect 输出中识别状态推进。
    fn advance(&self, app: &AppHandle, boot: u64, text: &str) {
        if self.generation.load(Ordering::SeqCst) != boot {
            return;
        }
        let lower = text.to_lowercase();
        let current = self.lock_state().state.clone();
        if is_tunnel_up_line(&lower) {
            if let Some(ip) = parse_tunnel_ip(text) {
                self.lock_state().tunnel_ip = Some(ip);
            }
            if current != "connected" {
                self.transition(app, ConnState::Connected, "隧道已建立");
            }
        } else if lower.contains("esp session established") {
            if current != "connected" {
                self.transition(app, ConnState::Connected, "隧道已建立（DTLS 直连）");
            }
        } else if lower.contains("cstp connected") || lower.contains("ssl negotiation with") {
            if current == "idle" {
                self.transition(app, ConnState::Handshaking, "SSL 握手进行中");
            }
        }
    }

    /// 从日志里捕捉服务器证书校验失败的信息。
    ///
    /// openconnect 在验证失败时会打印：
    ///   Certificate from VPN server "x" failed verification.
    ///   Reason: signer not found
    ///       --servercert pin-sha256:AbCd...
    ///   Enter 'yes' to accept, 'no' to abort;
    ///
    /// 我们不等它询问（stdin 已关闭），而是取出指纹交给用户确认，
    /// 确认后写入 profile.servercert 再重连。
    fn detect_cert_prompt(&self, app: &AppHandle, boot: u64, text: &str) {
        if self.generation.load(Ordering::SeqCst) != boot {
            return;
        }
        if let Some(reason) = text.split("Reason:").nth(1) {
            let mut slot = self.cert_reason.lock().unwrap_or_else(|e| e.into_inner());
            *slot = reason.trim().to_string();
        }
        let Some(idx) = text.find("pin-sha256:") else {
            return;
        };
        if self.cert_prompted.swap(true, Ordering::SeqCst) {
            return;
        }
        let pin = text[idx..]
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if pin.is_empty() {
            return;
        }
        let reason = self
            .cert_reason
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let profile_id = self.lock_state().profile_id.clone().unwrap_or_default();
        self.log(app, "app", "服务器证书未通过校验，等待用户确认是否信任该证书");
        let _ = app.emit(
            EVT_CERT,
            crate::model::CertPrompt {
                profile_id,
                reason,
                pin,
            },
        );
    }

    /// 启动 openconnect 子进程。
    ///
    /// `password` 为 Some 时通过 stdin 传入（配合 --passwd-on-stdin），不落盘、不进 argv。
    pub fn start(
        self: &Arc<Self>,
        app: &AppHandle,
        profile: &crate::model::Profile,
        bin: &Path,
        password: Option<String>,
    ) -> Result<u32, String> {
        // 串行化：并发调用时只允许一个进程被拉起
        let _guard = self
            .start_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if self.running_pid().is_some() {
            return Err("已有连接在进行中，请先断开".into());
        }
        if !bin.is_file() {
            return Err(format!(
                "openconnect 可执行文件不存在：{}。请在配置中指定正确路径。",
                bin.display()
            ));
        }

        let mut cmd = Command::new(bin);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // 强制英文输出：状态机（Connected as / CSTP connected）与故障分类均按
        // openconnect 的英文原文匹配。openconnect 带 gettext 翻译且翻译目录路径
        // 编译期写死，系统为中文时会输出「已配置为…」，导致状态停在「握手中」。
        // LANGUAGE 优先级高于 LC_ALL，必须一并清除。
        cmd.env("LC_ALL", "C")
            .env("LC_MESSAGES", "C")
            .env("LANG", "C")
            .env_remove("LANGUAGE");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // 不弹出控制台黑窗
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        // ---- 认证参数 ----
        if !profile.username.is_empty() {
            cmd.arg(format!("--user={}", profile.username));
        }
        match profile.auth_method {
            crate::model::AuthMethod::Password => {
                cmd.arg("--passwd-on-stdin");
            }
            crate::model::AuthMethod::Certificate => {
                cmd.arg(format!("--certificate={}", profile.certificate_path));
                if !profile.key_path.is_empty() {
                    cmd.arg(format!("--sslkey={}", profile.key_path));
                }
                if password.is_some() {
                    // 证书私钥口令同样走 stdin
                    cmd.arg("--passwd-on-stdin");
                }
            }
        }
        if !profile.protocol.is_empty() {
            cmd.arg(format!("--protocol={}", profile.protocol));
        }
        // 用户已确认过的服务器证书指纹：跳过 CA 校验但绑定到该证书，
        // 比无条件信任安全（证书更换后会再次询问）
        if !profile.servercert.is_empty() {
            cmd.arg(format!("--servercert={}", profile.servercert));
        }
        // 内置运行时会自带 vpnc-script，显式传入以免依赖系统默认路径
        // （openconnect 默认只在 /etc/vpnc/vpnc-script 与 PATH 中查找）
        if let Some(script) = resolve_vpnc_script(bin) {
            cmd.arg(format!("--script={}", script.display()));
        }
        // 内置运行时自带 CA bundle：GnuTLS 默认信任库是编译期写死的 Homebrew
        // 路径，在未安装 openconnect 的机器上会导致服务器证书校验失败。
        // 用户可通过额外参数里的 --cafile 覆盖此默认值。
        if let Some(ca) = sibling_file(bin, &["ca-bundle.pem", "ca-certificates.crt"]) {
            cmd.arg(format!("--cafile={}", ca.display()));
        }
        // ---- 用户自定义参数 ----
        for arg in split_args(&profile.extra_args) {
            cmd.arg(arg);
        }
        // ---- 网关（必须位于参数末尾）----
        cmd.arg(&profile.gateway);

        // 记录将要执行的命令行，便于运维排障（密码不在其中）
        self.log(
            app,
            "app",
            &format!("$ {} {}", bin.display(), render_args(&cmd)),
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| spawn_error_hint(&e, bin))?;

        let pid = child.id();
        let boot = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.pid.store(pid, Ordering::SeqCst);
        self.user_stop.store(false, Ordering::SeqCst);
        self.cert_prompted.store(false, Ordering::SeqCst);
        *self.cert_reason.lock().unwrap_or_else(|e| e.into_inner()) = String::new();

        // 写入凭据后立即关闭 stdin
        if let Some(mut stdin) = child.stdin.take() {
            let payload = password.unwrap_or_default();
            let write_result = if payload.is_empty() {
                Ok(())
            } else {
                stdin
                    .write_all(payload.as_bytes())
                    .and_then(|_| stdin.write_all(b"\n"))
                    .and_then(|_| stdin.flush())
            };
            if let Err(e) = write_result {
                self.log(app, "app", &format!("警告：写入凭据到 openconnect 失败：{e}"));
            }
            drop(stdin);
        }

        // 状态置为握手
        {
            let mut s = self.lock_state();
            s.profile_id = Some(profile.id.clone());
            s.profile_name = Some(profile.name.clone());
            s.gateway = Some(profile.gateway.clone());
            s.tunnel_ip = None;
        }
        self.transition(app, ConnState::Handshaking, "正在启动 openconnect…");

        // stdout / stderr 分流读取
        if let Some(out) = child.stdout.take() {
            pump(app.clone(), self.clone(), out, "stdout", boot);
        }
        if let Some(err) = child.stderr.take() {
            pump(app.clone(), self.clone(), err, "stderr", boot);
        }

        // 独立线程回收子进程，捕获异常退出
        let waiter_app = app.clone();
        let waiter_self = self.clone();
        std::thread::spawn(move || {
            let mut child = child;
            let status = child.wait();
            // 只有当前这一代进程才有权改变状态
            if waiter_self.generation.load(Ordering::SeqCst) != boot {
                return;
            }
            waiter_self.pid.store(0, Ordering::SeqCst);
            let by_user = waiter_self.user_stop.swap(false, Ordering::SeqCst);
            match status {
                Ok(st) => {
                    let code = st.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
                    if by_user {
                        waiter_self.log(&waiter_app, "app", "openconnect 进程已终止");
                        waiter_self.transition(&waiter_app, ConnState::Idle, "已断开");
                    } else if st.success() {
                        waiter_self.log(&waiter_app, "app", "openconnect 正常退出");
                        waiter_self.transition(&waiter_app, ConnState::Idle, "连接已结束");
                    } else {
                        waiter_self.log(
                            &waiter_app,
                            "app",
                            &format!("openconnect 异常退出，退出码 {code}（请查看上方日志定位原因）"),
                        );
                        waiter_self.log(&waiter_app, "app", EXIT_CODE_HINT);
                        waiter_self.transition(
                            &waiter_app,
                            ConnState::Error,
                            &format!("连接异常断开（退出码 {code}）"),
                        );
                    }
                }
                Err(e) => {
                    waiter_self.log(&waiter_app, "app", &format!("等待子进程结束失败：{e}"));
                    waiter_self.transition(&waiter_app, ConnState::Error, "无法获取进程退出状态");
                }
            }
        });

        Ok(pid)
    }

    /// 终止连接。`hard=true` 立即 SIGKILL / taskkill /F。
    pub fn stop(self: &Arc<Self>, app: &AppHandle, hard: bool) -> Result<(), String> {
        let pid = match self.running_pid() {
            Some(p) => p,
            None => return Err("当前没有运行中的 openconnect 进程".into()),
        };
        self.user_stop.store(true, Ordering::SeqCst);
        self.transition(app, ConnState::Disconnecting, "正在断开连接…");
        self.log(
            app,
            "app",
            if hard {
                "强制终止 openconnect 进程（SIGKILL / taskkill /F）"
            } else {
                "请求 openconnect 主动断开（SIGTERM）"
            },
        );
        if let Err(e) = terminate(pid, hard) {
            // 终止失败，回滚「用户主动停止」标记，避免掩盖后续异常退出原因
            self.user_stop.store(false, Ordering::SeqCst);
            self.transition(app, ConnState::Connected, "终止请求失败，进程仍在运行");
            return Err(e);
        }

        if !hard {
            // 优雅退出宽限 6 秒，超时后强制回收，防止进程卡死残留
            let this = self.clone();
            let app2 = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(6));
                if this.running_pid() == Some(pid) {
                    this.log(&app2, "app", "6 秒内未退出，升级为强制终止");
                    let _ = terminate(pid, true);
                }
            });
        }
        Ok(())
    }
}

/// 查找与 openconnect 同目录的附带文件（数据文件，不需要可执行位）。
fn sibling_file(bin: &Path, names: &[&str]) -> Option<PathBuf> {
    let dir = bin.parent()?;
    names
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
}

/// 定位 openconnect 同目录下的 vpnc-script。
///
/// - 类 Unix 用 `vpnc-script`，Windows 用 `vpnc-script-win.js`；
/// - openconnect 内部以 shell 拼接命令执行该脚本，路径含空格会失败。
///   `.app` 的名字通常是 "Octauri.app"，必然带空格，因此这种情况
///   复制一份到无空格的临时目录再传入。
fn resolve_vpnc_script(bin: &Path) -> Option<PathBuf> {
    let dir = bin.parent()?;
    for name in ["vpnc-script", "vpnc-script-win.js"] {
        let src = dir.join(name);
        if !src.is_file() {
            continue;
        }
        if !src.to_string_lossy().contains(' ') {
            return Some(src);
        }
        let tmp_dir = std::env::temp_dir().join("octauri-scripts");
        if std::fs::create_dir_all(&tmp_dir).is_err() {
            // 无法创建临时目录时退回原路径（可能失败，但至少不是静默丢参数）
            return Some(src);
        }
        let dst = tmp_dir.join(name);
        if std::fs::copy(&src, &dst).is_err() {
            return Some(src);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755));
        }
        return Some(dst);
    }
    None
}

fn pump<R: Read + Send + 'static>(
    app: AppHandle,
    runner: Arc<Runner>,
    reader: R,
    stream: &'static str,
    boot: u64,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(text) = line else { break };
            if runner.generation.load(Ordering::SeqCst) != boot {
                break;
            }
            if text.trim().is_empty() {
                continue;
            }
            runner.log(&app, stream, &text);
            runner.advance(&app, boot, &text);
            runner.detect_cert_prompt(&app, boot, &text);
        }
    });
}

/// 隧道建立标志行。openconnect 不同版本措辞不同：
///   旧版：  "Connected as 10.x.x.x, using SSL, with DTLS disabled"
///   9.x：   "Configured as 10.x.x.x, with SSL connected and DTLS disabled"
const TUNNEL_UP_PREFIXES: [&str; 2] = ["connected as ", "configured as "];

fn is_tunnel_up_line(lower: &str) -> bool {
    TUNNEL_UP_PREFIXES.iter().any(|k| lower.contains(k))
}

/// 从隧道建立标志行中提取分配到的 IP。
fn parse_tunnel_ip(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let idx = TUNNEL_UP_PREFIXES
        .iter()
        .find_map(|k| lower.find(k).map(|i| i + k.len()))?;
    let ip: String = text[idx..]
        .chars()
        .take_while(|c| c.is_ascii_hexdigit() || *c == '.' || *c == ':')
        .collect();
    (!ip.is_empty()).then_some(ip)
}

/// 故障关键词分类，供前端着色与排障过滤。
fn classify(text: &str) -> (&'static str, Option<&'static str>) {
    let l = text.to_lowercase();
    let has = |k: &str| l.contains(k);

    // 证书类
    if has("certificate verify failed")
        || has("unable to verify")
        || has("self-signed")
        || has("self signed")
        || has("certificate has expired")
        || has("ssl certificate")
    {
        return ("error", Some("cert"));
    }
    // 认证类
    if has("login failed")
        || has("authentication failed")
        || has("authentication failure")
        || has("invalid password")
        || has("auth failed")
        || has("http 401")
        || has("401 unauthorized")
    {
        return ("error", Some("auth"));
    }
    // 握手/超时类
    if has("handshake failed")
        || has("handshake timeout")
        || has("connection timed out")
        || has("connect timeout")
        || has("network is unreachable")
        || has("no route to host")
        || has("vpn service unavailable")
    {
        return ("error", Some("handshake"));
    }
    // 成功标志：必须先于 DTLS 判断。成功行形如
    // "Connected as 10.x, using SSL, with DTLS disabled"，含 "dtls" 字样，
    // 若后判断会被误归为 DTLS 信息行而不显示为成功。
    if is_tunnel_up_line(&l) || has("esp session established") {
        return ("success", None);
    }
    // DTLS 相关（降级为警告，不阻断连接）
    if has("dtls") {
        if has("failed") || has("not") || has("downgrade") || has("fallback") {
            return ("warn", Some("dtls"));
        }
        return ("info", Some("dtls"));
    }
    // 路由 / DNS。vpnc-script 在 macOS 上输出 "add net x: gateway y"，
    // 在 Linux 上为 "add route"
    if has("add route")
        || has("add net ")
        || has("add host ")
        || has("route to")
        || has("default route")
    {
        return ("info", Some("route"));
    }
    if has("dns") && (has("fail") || has("error")) {
        return ("warn", Some("dns"));
    }
    if has("failed") || has("error") || has("fatal") {
        return ("error", None);
    }
    if has("warning") || has("warn:") {
        return ("warn", None);
    }
    ("info", None)
}

/// 生成带引号的命令行字符串（仅用于日志展示）。
fn render_args(cmd: &Command) -> String {
    cmd.get_args()
        .map(|a| {
            let s = a.to_string_lossy();
            if s.contains(' ') {
                format!("\"{s}\"")
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 按 shell 习惯切分自定义参数，支持单/双引号。
pub fn split_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has_content = false;
    for ch in input.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    cur.push(ch);
                }
            }
            None => {
                if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                    has_content = true;
                } else if ch.is_whitespace() {
                    if has_content || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        has_content = false;
                    }
                } else {
                    cur.push(ch);
                }
            }
        }
    }
    if has_content || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(unix)]
fn terminate(pid: u32, hard: bool) -> Result<(), String> {
    let sig = if hard { libc::SIGKILL } else { libc::SIGTERM };
    // SAFETY: kill 仅向指定 pid 发送信号，参数为合法整型。
    let rc = unsafe { libc::kill(pid as libc::pid_t, sig) };
    if rc == 0 {
        Ok(())
    } else {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            // 进程已不存在，视为成功
            Ok(())
        } else {
            Err(format!("终止进程 {pid} 失败：{err}"))
        }
    }
}

#[cfg(windows)]
fn terminate(pid: u32, hard: bool) -> Result<(), String> {
    // Windows 下 openconnect 为控制台程序，软终止通常无效，但仍先尝试 /T 终止进程树。
    let mut cmd = Command::new("taskkill");
    cmd.arg("/PID").arg(pid.to_string()).arg("/T");
    if hard {
        cmd.arg("/F");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("执行 taskkill 失败：{e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // 进程已结束也视为成功
        if msg.to_lowercase().contains("not found")
            || msg.contains("找不到")
            || msg.contains("没有找到")
        {
            Ok(())
        } else {
            Err(format!("终止进程失败：{msg}"))
        }
    }
}

/// 把 std::io::Error 翻译成运维人员能直接照做的提示。
fn spawn_error_hint(e: &std::io::Error, bin: &Path) -> String {
    use std::io::ErrorKind;
    let base = format!("启动 openconnect 失败（{}）：{e}", bin.display());
    let hint = match e.kind() {
        ErrorKind::PermissionDenied => {
            if cfg!(windows) {
                "→ 权限不足。请以管理员身份运行本程序。"
            } else {
                "→ 权限不足。openconnect 需要 root 权限，请用 sudo 启动本程序。"
            }
        }
        ErrorKind::NotFound => "→ 可执行文件不存在，请在配置中重新指定 openconnect 路径。",
        _ => {
            if cfg!(windows) {
                "→ 请检查是否缺失依赖 DLL，或被杀毒软件拦截。"
            } else {
                "→ 请检查该文件是否具备可执行权限。"
            }
        }
    };
    format!("{base}\n{hint}")
}

/// openconnect 退出码速查（README 与关于页同步引用）。
pub const EXIT_CODE_HINT: &str =
    "openconnect 退出码：1=参数错误，2=连接/认证失败；其余见 openconnect 手册。";
#[cfg(test)]
mod tests {
    use super::classify;

    /// 回归用例取自真实网关日志（英文环境下的 openconnect 9.21 输出）。
    #[test]
    fn classifies_real_anyconnect_session() {
        assert_eq!(
            classify("Server certificate verify failed: signer not found"),
            ("error", Some("cert"))
        );
        assert_eq!(
            classify("Connected as 10.255.225.16, using SSL, with DTLS disabled"),
            ("success", None)
        );
        assert_eq!(
            classify("Failed to connect DTLS tunnel; using HTTPS instead"),
            ("warn", Some("dtls"))
        );
        assert_eq!(classify("add net 10.0.0.0: gateway 10.255.225.16").1, Some("route"));
        assert_eq!(classify("Login failed.").1, Some("auth"));
    }

    /// 中文输出不应被误判为成功——这正是本次缺陷的成因，
    /// 修复方式是在子进程层面强制 LC_ALL=C，而不是让分类器去适配多语言。
    #[test]
    fn classifies_openconnect_921_configured_line() {
        assert_eq!(
            classify("Configured as 10.255.225.16, with SSL connected and DTLS disabled").0,
            "success"
        );
    }

    #[test]
    fn parses_tunnel_ip() {
        use super::parse_tunnel_ip;
        assert_eq!(
            parse_tunnel_ip("Connected as 10.255.225.16, using SSL, with DTLS disabled").as_deref(),
            Some("10.255.225.16")
        );
        // openconnect 9.21 实际输出（取自用户网关日志原文）
        assert_eq!(
            parse_tunnel_ip("Configured as 10.255.225.16, with SSL connected and DTLS disabled")
                .as_deref(),
            Some("10.255.225.16")
        );
        assert_eq!(parse_tunnel_ip("CSTP connected. DPD 30"), None);
    }

    #[test]
    fn localized_output_is_not_recognized() {
        assert_ne!(classify("已配置为 10.255.225.16，使用 SSL 已连接").0, "success");
    }
}
