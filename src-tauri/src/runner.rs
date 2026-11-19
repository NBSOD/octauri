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
    /// root 辅助进程连接（macOS）。GUI 以普通用户运行时，
    /// openconnect 由它以 root 拉起，日志经它回传。
    #[cfg(target_os = "macos")]
    helper: Mutex<Option<crate::helper::HelperLink>>,
    /// 辅助进程回报的错误（EXEC 失败等），供 start 同步读取
    helper_err: Mutex<Option<String>>,
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
            #[cfg(target_os = "macos")]
            helper: Mutex::new(None),
            helper_err: Mutex::new(None),
        })
    }

    #[cfg(target_os = "macos")]
    fn lock_helper(&self) -> MutexGuard<'_, Option<crate::helper::HelperLink>> {
        self.helper.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 是否已接入 root 辅助进程（仅 macOS 使用；其他平台恒为 false，
    /// 但保留该方法以便共享代码路径编译）。
    pub fn has_helper(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.lock_helper().is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn clear_helper_error(&self) {
        *self.helper_err.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// 当前是否记录了辅助进程错误（供前端恢复状态时读取）
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn helper_error(&self) -> Option<String> {
        self.helper_err
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn take_helper_error(&self) -> Option<String> {
        self.helper_err
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    fn set_helper_error(&self, msg: String) {
        *self.helper_err.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
    }

    /// 处理一行 openconnect 输出：写日志、推进状态、检测证书提示。
    /// 本地 pipe 与辅助进程两条路径共用，避免行为分叉。
    fn handle_output_line(&self, app: &AppHandle, boot: u64, stream: &str, text: &str) {
        if self.generation.load(Ordering::SeqCst) != boot {
            return;
        }
        if text.trim().is_empty() {
            return;
        }
        self.log(app, stream, text);
        self.advance(app, boot, text);
        self.detect_cert_prompt(app, boot, text);
    }

    /// 接入 root 辅助进程：保存写端，并起一条线程处理它回传的事件。
    #[cfg(target_os = "macos")]
    pub fn attach_helper(
        self: &Arc<Self>,
        app: AppHandle,
        link: crate::helper::HelperLink,
    ) -> Result<(), String> {
        let read_stream = link.clone_stream()?;
        *self.lock_helper() = Some(link);
        let runner = self.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(read_stream);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let mut it = line.splitn(3, ' ');
                match it.next() {
                    Some("PID") => {
                        if let Some(pid) = it.next().and_then(|v| v.parse::<u32>().ok()) {
                            runner.pid.store(pid, Ordering::SeqCst);
                            runner.transition(&app, ConnState::Handshaking, "正在启动 openconnect…");
                        }
                    }
                    Some("O") => {
                        let which = it.next().unwrap_or("out");
                        let text = it
                            .next()
                            .and_then(crate::helper::b64_decode)
                            .map(|v| String::from_utf8_lossy(&v).into_owned())
                            .unwrap_or_default();
                        let stream = if which == "err" { "stderr" } else { "stdout" };
                        let boot = runner.generation.load(Ordering::SeqCst);
                        runner.handle_output_line(&app, boot, stream, &text);
                    }
                    Some("EXIT") => {
                        let code = it.next().and_then(|v| v.parse::<i32>().ok()).unwrap_or(-1);
                        let boot = runner.generation.load(Ordering::SeqCst);
                        if runner.generation.load(Ordering::SeqCst) != boot {
                            continue;
                        }
                        runner.finish_session(&app, code, code == 0);
                    }
                    Some("ERR") => {
                        let msg = it
                            .next()
                            .and_then(crate::helper::b64_decode)
                            .map(|v| String::from_utf8_lossy(&v).into_owned())
                            .unwrap_or_else(|| "辅助进程执行失败".into());
                        runner.log(&app, "app", &format!("辅助进程：{msg}"));
                        runner.set_helper_error(msg);
                    }
                    _ => {}
                }
            }
            // 辅助进程断开：若仍有连接，视为异常
            runner.pid.store(0, Ordering::SeqCst);
            if let Ok(mut slot) = runner.helper.lock() {
                *slot = None;
            }
            if !matches!(runner.lock_state().state.as_str(), "idle" | "error") {
                runner.log(&app, "app", "与辅助进程的连接已断开");
                runner.transition(&app, ConnState::Error, "辅助进程已退出");
            }
        });
        Ok(())
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

        let argv = build_argv(profile, bin, password.is_some());

        // 记录将要执行的命令行，便于运维排障（密码不在其中）
        self.log(
            app,
            "app",
            &format!("$ {} {}", bin.display(), render_args(&argv)),
        );

        // macOS：优先交给 root 辅助进程。GUI 自身以普通用户运行，
        // 这样 Dock 退出 / Cmd+Q / 强制退出都不会因 root 身份而失效。
        #[cfg(target_os = "macos")]
        {
            if self.has_helper() {
                return self.start_via_helper(app, profile, bin, &argv, password);
            }
            // 未授权时直接给出原因，而不是让 openconnect 以普通用户身份失败
            // 自测连接（--spawn）用于验证 helper 链路，未就绪时也允许走本地路径
            if !crate::args::no_elevate() && crate::args::spawn().is_none() {
                return Err(
                    "尚未获得管理员授权。\n退出 Octauri 后重新打开，并在系统弹窗中输入密码允许。"
                        .into(),
                );
            }
        }

        self.start_local(app, profile, bin, &argv, password)
    }

    /// 本地直接拉起 openconnect（Windows / Linux，以及 macOS 未启用辅助进程时）。
    fn start_local(
        self: &Arc<Self>,
        app: &AppHandle,
        profile: &crate::model::Profile,
        bin: &Path,
        argv: &[String],
        password: Option<String>,
    ) -> Result<u32, String> {
        let mut cmd = Command::new(bin);
        cmd.args(argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // 强制英文输出：状态机（Connected as / Configured as）与故障分类均按
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

        let mut child = cmd.spawn().map_err(|e| spawn_error_hint(&e, bin))?;

        let pid = child.id();
        // 必须立即记录 pid：stop()/shutdown() 都以 running_pid() 判断是否有子进程。
        // 早期版本只在 macOS 辅助进程回传 PID 时才写入，导致 Windows/Linux 上
        // 「断开」「强制终止」永远报“没有运行中的进程”，退出时也清理不掉 openconnect。
        self.pid.store(pid, Ordering::SeqCst);
        let boot = self.begin_session(profile);

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
            match status {
                Ok(st) => waiter_self.finish_session(
                    &waiter_app,
                    st.code().unwrap_or(-1),
                    st.success(),
                ),
                Err(e) => {
                    waiter_self.pid.store(0, Ordering::SeqCst);
                    waiter_self.log(&waiter_app, "app", &format!("等待子进程结束失败：{e}"));
                    waiter_self.transition(&waiter_app, ConnState::Error, "无法获取进程退出状态");
                }
            }
        });

        Ok(pid)
    }

    /// 通过 root 辅助进程拉起 openconnect（macOS）。
    #[cfg(target_os = "macos")]
    fn start_via_helper(
        self: &Arc<Self>,
        app: &AppHandle,
        profile: &crate::model::Profile,
        bin: &Path,
        argv: &[String],
        password: Option<String>,
    ) -> Result<u32, String> {
        // 第一个参数必须是可执行文件本身：辅助进程据此 spawn
        let mut line = format!("EXEC {}", crate::helper::b64_encode(bin.to_string_lossy().as_bytes()));
        for a in argv {
            line.push(' ');
            line.push_str(&crate::helper::b64_encode(a.as_bytes()));
        }
        {
            let mut slot = self.lock_helper();
            let Some(link) = slot.as_mut() else {
                return Err("辅助进程未就绪，请重新授权".into());
            };
            link.send(&line)?;
            if let Some(pw) = password.as_deref().filter(|s| !s.is_empty()) {
                link.send(&format!(
                    "STDIN {}",
                    crate::helper::b64_encode(pw.as_bytes())
                ))?;
            } else {
                link.send("STDIN ")?;
            }
        }

        self.clear_helper_error();
        // generation 由 begin_session 推进；读线程以它判断输出归属
        let _boot = self.begin_session(profile);

        // 等辅助进程回报 pid（或错误）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if let Some(err) = self.take_helper_error() {
                self.pid.store(0, Ordering::SeqCst);
                self.transition(app, ConnState::Idle, "未连接");
                return Err(err);
            }
            if let Some(pid) = self.running_pid() {
                return Ok(pid);
            }
            if std::time::Instant::now() >= deadline {
                self.pid.store(0, Ordering::SeqCst);
                self.transition(app, ConnState::Idle, "未连接");
                return Err("辅助进程未响应（15 秒超时）".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    }

    /// 一次连接开始时的公共初始化，返回本次的 generation。
    fn begin_session(&self, profile: &crate::model::Profile) -> u64 {
        let boot = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.user_stop.store(false, Ordering::SeqCst);
        self.cert_prompted.store(false, Ordering::SeqCst);
        *self.cert_reason.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
        {
            let mut s = self.lock_state();
            s.profile_id = Some(profile.id.clone());
            s.profile_name = Some(profile.name.clone());
            s.gateway = Some(profile.gateway.clone());
            s.tunnel_ip = None;
        }
        boot
    }

    /// 连接结束时的公共收尾（本地 waiter 与 helper 读线程共用）。
    fn finish_session(&self, app: &AppHandle, code: i32, success: bool) {
        self.pid.store(0, Ordering::SeqCst);
        let by_user = self.user_stop.swap(false, Ordering::SeqCst);
        let code_text = if code < 0 {
            "signal".to_string()
        } else {
            code.to_string()
        };
        if by_user {
            self.log(app, "app", "openconnect 进程已终止");
            self.transition(app, ConnState::Idle, "已断开");
        } else if success {
            self.log(app, "app", "openconnect 正常退出");
            self.transition(app, ConnState::Idle, "连接已结束");
        } else {
            self.log(
                app,
                "app",
                &format!("openconnect 异常退出，退出码 {code_text}（请查看上方日志定位原因）"),
            );
            self.log(app, "app", EXIT_CODE_HINT);
            self.transition(
                app,
                ConnState::Error,
                &format!("连接异常断开（退出码 {code_text}）"),
            );
        }
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
                "强制终止 openconnect 进程（Windows: taskkill /F / Unix: SIGKILL）"
            } else if cfg!(windows) {
                "请求 openconnect 主动断开（Windows: 发送 Ctrl+C）"
            } else {
                "请求 openconnect 主动断开（SIGTERM）"
            },
        );
        // macOS：openconnect 由 root 辅助进程拉起，本地进程无权发信号
        #[cfg(target_os = "macos")]
        if self.has_helper() {
            let send = {
                let mut slot = self.lock_helper();
                match slot.as_mut() {
                    Some(link) => link.send(&format!("SIG {}", if hard { "kill" } else { "term" })),
                    None => Err("辅助进程连接已断开".to_string()),
                }
            };
            if let Err(e) = send {
                self.user_stop.store(false, Ordering::SeqCst);
                self.transition(app, ConnState::Connected, "终止请求失败，进程仍在运行");
                return Err(e);
            }
            if !hard {
                // 6 秒宽限后仍存活则强制终止，防止卡死残留
                let this = self.clone();
                let app2 = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(6));
                    if this.running_pid() == Some(pid) {
                        this.log(&app2, "app", "6 秒内未退出，升级为强制终止");
                        if let Some(link) = this.lock_helper().as_mut() {
                            let _ = link.send("SIG kill");
                        }
                    }
                });
            }
            return Ok(());
        }

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

    /// 程序退出前同步清理子进程（阻塞，最多约 1.5 秒）。
    ///
    /// openconnect 是独立进程，GUI 退出时不会随之结束：它会变成孤儿进程，
    /// 隧道、路由、DNS 全部保留，而且以 root 运行，用户只能 sudo kill 清理。
    /// 先发 SIGTERM，让 openconnect 调用 vpnc-script 还原路由与 DNS；
    /// 超时后再发 SIGKILL。
    pub fn shutdown(&self) {
        // macOS：openconnect 归 root 辅助进程管，本地不能直接发信号。
        // 通知它退出，它会先结束 openconnect（还原路由与 DNS）再退出；
        // 即使这里不等它，GUI 退出导致 socket 关闭也会让它走同一清理路径。
        #[cfg(target_os = "macos")]
        if let Some(link) = self.lock_helper().as_mut() {
            let _ = link.send("QUIT");
            std::thread::sleep(std::time::Duration::from_millis(400));
            return;
        }

        let Some(pid) = self.running_pid() else {
            return;
        };
        self.user_stop.store(true, Ordering::SeqCst);
        let _ = terminate(pid, false);
        // 等待回收线程把 pid 清零（即子进程已退出）。
        // Unix：SIGTERM 后约 90ms 退出；Windows：Ctrl+C 后还要跑 vpnc-script
        // （cscript 执行 JS），实测要 1~2 秒，所以这里给 3 秒。
        #[cfg(windows)]
        let wait_rounds = 30;
        #[cfg(not(windows))]
        let wait_rounds = 12;
        for _ in 0..wait_rounds {
            if self.running_pid() != Some(pid) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = terminate(pid, true);
        std::thread::sleep(std::time::Duration::from_millis(200));
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
            runner.handle_output_line(&app, boot, stream, &text);
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
fn render_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 构造 openconnect 的命令行参数（不含可执行文件本身）。
///
/// 本地拉起与交root辅助进程执行共用，保证两条路径参数一致。
fn build_argv(
    profile: &crate::model::Profile,
    bin: &Path,
    has_password: bool,
) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    if !profile.username.is_empty() {
        a.push(format!("--user={}", profile.username));
    }
    match profile.auth_method {
        crate::model::AuthMethod::Password => a.push("--passwd-on-stdin".into()),
        crate::model::AuthMethod::Certificate => {
            a.push(format!("--certificate={}", profile.certificate_path));
            if !profile.key_path.is_empty() {
                a.push(format!("--sslkey={}", profile.key_path));
            }
            if has_password {
                // 证书私钥口令同样走 stdin
                a.push("--passwd-on-stdin".into());
            }
        }
    }
    if !profile.protocol.is_empty() {
        a.push(format!("--protocol={}", profile.protocol));
    }
    // 用户已确认过的服务器证书指纹：跳过 CA 校验但绑定到该证书，
    // 比无条件信任安全（证书更换后会再次询问）
    if !profile.servercert.is_empty() {
        a.push(format!("--servercert={}", profile.servercert));
    }
    // 内置运行时会自带 vpnc-script，显式传入以免依赖系统默认路径
    // （openconnect 默认只在 /etc/vpnc/vpnc-script 与 PATH 中查找）
    if let Some(script) = resolve_vpnc_script(bin) {
        a.push(format!("--script={}", script.display()));
    }
    // 内置运行时自带 CA bundle：GnuTLS 默认信任库是编译期写死的 Homebrew
    // 路径，在未安装 openconnect 的机器上会导致服务器证书校验失败。
    // 用户可通过额外参数里的 --cafile 覆盖此默认值。
    if let Some(ca) = sibling_file(bin, &["ca-bundle.pem", "ca-certificates.crt"]) {
        a.push(format!("--cafile={}", ca.display()));
    }
    a.extend(split_args(&profile.extra_args));
    // 网关必须位于参数末尾
    a.push(profile.gateway.clone());
    a
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
fn send_ctrl_c(pid: u32) -> Result<(), String> {
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler, CTRL_C_EVENT,
    };

    // 本进程是 GUI 子系统，没有控制台，因此可以“附身”到目标进程的隐藏控制台。
    if unsafe { AttachConsole(pid) } == 0 {
        return Err(format!("AttachConsole({pid}) 失败"));
    }
    // 事件会发给该控制台上的所有进程；先让本进程忽略 Ctrl+C，避免把自己结束掉。
    unsafe { SetConsoleCtrlHandler(None, 1) };
    let sent = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) } != 0;
    unsafe { FreeConsole() };
    unsafe { SetConsoleCtrlHandler(None, 0) };

    if sent {
        Ok(())
    } else {
        Err("GenerateConsoleCtrlEvent 失败".into())
    }
}

#[cfg(windows)]
fn terminate(pid: u32, hard: bool) -> Result<(), String> {
    // 优雅断开：openconnect 是控制台程序，收 Ctrl+C 后会走正常流程，
    // 调用 vpnc-script 还原路由与 DNS。不能只靠 taskkill（不带 /F 对控制台
    // 进程无效，而且它输出的是本地化文本，按 UTF-8 解码就是乱码）。
    if !hard && send_ctrl_c(pid).is_ok() {
        return Ok(());
    }

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
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("执行 taskkill 失败：{e}"))?;

    if out.status.success() {
        return Ok(());
    }
    if hard {
        // 只报退出码，绝不把 taskkill 的本地化 stderr 拼进界面（会乱码）。
        return Err(format!(
            "无法强制结束进程 {pid}（taskkill 退出码 {:?}）",
            out.status.code()
        ));
    }
    // 软终止失败（进程已退出 / 没有控制台可挂）：不弹错，交给 stop() 与
    // shutdown() 里的兜底逻辑在宽限期后升级为 taskkill /F。
    Ok(())
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
    /// 模拟 spawn + 回收线程，返回子进程 pid 与「是否已退出」探针
    #[cfg(unix)]
    fn spawn_tracked(runner: &std::sync::Arc<super::Runner>, script: &str) -> u32 {
        use std::sync::atomic::Ordering;
        let mut child = std::process::Command::new("sh").arg("-c").arg(script).spawn().unwrap();
        let pid = child.id();
        runner.pid.store(pid, Ordering::SeqCst);
        let r = runner.clone();
        std::thread::spawn(move || {
            let _ = child.wait();
            r.pid.store(0, Ordering::SeqCst);
        });
        pid
    }

    #[cfg(unix)]
    fn alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    /// 正常响应 SIGTERM 的子进程：shutdown 应很快返回且进程已结束
    #[cfg(unix)]
    #[test]
    fn shutdown_terminates_cooperative_child() {
        let _g = crate::test_lock();
        let runner = super::Runner::new();
        let pid = spawn_tracked(&runner, "exec sleep 30");
        let t = std::time::Instant::now();
        runner.shutdown();
        assert!(!alive(pid), "子进程应已退出");
        assert!(t.elapsed().as_millis() < 1500, "不应等到超时");
        assert_eq!(runner.running_pid(), None);
    }

    /// 忽略 SIGTERM 的子进程（模拟卡死的 openconnect）：超时后应被 SIGKILL
    #[cfg(unix)]
    #[test]
    fn shutdown_kills_child_ignoring_sigterm() {
        let _g = crate::test_lock();
        let runner = super::Runner::new();
        let pid = spawn_tracked(&runner, "trap '' TERM; while :; do sleep 0.1; done");
        std::thread::sleep(std::time::Duration::from_millis(200));
        runner.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(!alive(pid), "忽略 SIGTERM 的子进程应被强制结束");
    }

    #[test]
    fn shutdown_without_child_is_noop() {
        super::Runner::new().shutdown();
    }

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
