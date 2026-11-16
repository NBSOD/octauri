//! root 辅助进程（仅 macOS）：以 root 拉起 openconnect，并通过本机回环 socket
//! 把 I/O 转发给 GUI。
//!
//! 为什么需要：openconnect 建虚拟网卡需要 root。早期做法是把**整个 GUI** 用
//! `osascript ... with administrator privileges` 提权，代价是 macOS 无法正常终止
//! root 身份的 GUI —— Dock 退出、强制退出、Cmd+Q 都可能被忽略，进程留在后台。
//! 现在 GUI 以普通用户运行，只有本文件描述的辅助进程是 root，退出行为恢复正常。
//!
//! 连接方向是「GUI 监听 → helper 连回」，这样 GUI 不需要对外暴露任何可执行接口：
//!
//! ```text
//! GUI(普通用户)                    helper(root)
//!    bind 127.0.0.1:0  ──────►  连回该端口
//!                                读命令、spawn openconnect、转发输出
//! ```
//!
//! 握手校验用一次性 token（经临时文件传递，不进 argv，避免被 `ps` 读到）。
//! GUI 断连（正常退出或被强杀）时 helper 读到 EOF，会先结束 openconnect 再退出，
//! 因此不会留下孤儿进程。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

pub const HELO: &str = "OCTAURI-HELPER/1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

// ------------------------------ base64 ------------------------------
// 参数与日志都可能是任意 UTF-8，用 base64 放进单行协议里最省事，
// 这里手写编解码以免为此增加依赖。

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut vals = Vec::with_capacity(s.len());
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => continue,
            _ => return None,
        };
        vals.push(v);
    }
    let mut out = Vec::with_capacity(vals.len() * 3 / 4);
    for chunk in vals.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let mut n = 0u32;
        for (i, v) in chunk.iter().enumerate() {
            n |= u32::from(*v) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

// ------------------------------ helper 侧 ------------------------------

/// helper 进程入口。返回后进程退出。
pub fn helper_main(connect_port: u16, token_file: Option<String>) -> ! {
    let token = token_file
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if let Some(p) = token_file {
        // 读完即删：token 只在启动瞬间存在
        let _ = std::fs::remove_file(p);
    }

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, connect_port));
    let stream = match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[helper] 无法连接 GUI（{addr}）：{e}");
            std::process::exit(2);
        }
    };
    let _ = stream.set_nodelay(true);
    let Ok(write_half) = stream.try_clone() else {
        std::process::exit(2);
    };
    let out = std::sync::Arc::new(std::sync::Mutex::new(write_half));

    let send = {
        let out = out.clone();
        move |line: String| {
            if let Ok(mut w) = out.lock() {
                let _ = w.write_all(line.as_bytes());
                let _ = w.write_all(b"\n");
                let _ = w.flush();
            }
        }
    };

    send(format!("{HELO} {token}"));

    let mut reader = BufReader::new(stream);
    // 当前子进程：只保留 pid 与 stdin，退出由独立线程回收并回报，
    // 否则主循环会阻塞在读命令上、无法及时把退出码送回 GUI。
    let current: std::sync::Arc<std::sync::Mutex<Option<u32>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut child_stdin: Option<std::process::ChildStdin> = None;
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break, // GUI 关闭连接（含 GUI 被强杀）
            Ok(_) => {}
            Err(_) => break,
        }
        let msg = line.trim_end();
        if msg.is_empty() {
            continue;
        }
        let (cmd, rest) = msg.split_once(' ').unwrap_or((msg, ""));

        match cmd {
            "EXEC" => {
                // 已有子进程时先清理，保证同一时刻只有一个连接
                if let Some(pid) = current.lock().ok().and_then(|g| *g) {
                    #[cfg(unix)]
                    unsafe {
                        libc::kill(pid as libc::pid_t, libc::SIGKILL);
                    }
                }
                let argv: Vec<String> = rest
                    .split(' ')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| b64_decode(s))
                    .filter_map(|v| String::from_utf8(v).ok())
                    .collect();
                let Some((bin, args)) = argv.split_first() else {
                    send(format!("ERR {}", b64_encode("EXEC 缺少可执行文件".as_bytes())));
                    continue;
                };
                let mut c = std::process::Command::new(bin);
                c.args(args)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    // 与 GUI 本地拉起保持一致的英文输出环境
                    .env("LC_ALL", "C")
                    .env("LC_MESSAGES", "C")
                    .env("LANG", "C")
                    .env_remove("LANGUAGE");
                match c.spawn() {
                    Ok(mut child) => {
                        let pid = child.id();
                        child_stdin = child.stdin.take();
                        *current.lock().unwrap_or_else(|e| e.into_inner()) = Some(pid);
                        send(format!("PID {pid}"));

                        // stdout / stderr 分流回传
                        for (which, pipe) in [
                            (
                                "out",
                                child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
                            ),
                            (
                                "err",
                                child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>),
                            ),
                        ] {
                            if let Some(pipe) = pipe {
                                let out = out.clone();
                                std::thread::spawn(move || {
                                    let mut r = BufReader::new(pipe);
                                    let mut buf = Vec::new();
                                    loop {
                                        buf.clear();
                                        match r.read_until(b'\n', &mut buf) {
                                            Ok(0) => break,
                                            Ok(_) => {}
                                            Err(_) => break,
                                        }
                                        while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
                                            buf.pop();
                                        }
                                        let text = String::from_utf8_lossy(&buf).into_owned();
                                        if let Ok(mut w) = out.lock() {
                                            let _ = writeln!(
                                                w,
                                                "O {which} {}",
                                                b64_encode(text.as_bytes())
                                            );
                                            let _ = w.flush();
                                        }
                                    }
                                });
                            }
                        }

                        // 独立线程等待退出并回报
                        let out2 = out.clone();
                        let current2 = current.clone();
                        std::thread::spawn(move || {
                            let status = child.wait();
                            let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                            {
                                let mut slot =
                                    current2.lock().unwrap_or_else(|e| e.into_inner());
                                if *slot == Some(pid) {
                                    *slot = None;
                                }
                            }
                            if let Ok(mut w) = out2.lock() {
                                let _ = writeln!(w, "EXIT {code}");
                                let _ = w.flush();
                            }
                        });
                    }
                    Err(e) => {
                        send(format!("ERR {}", b64_encode(e.to_string().as_bytes())));
                    }
                }
            }
            "STDIN" => {
                if let Some(mut si) = child_stdin.take() {
                    if let Some(data) = b64_decode(rest) {
                        if !data.is_empty() {
                            let _ = si.write_all(&data);
                            let _ = si.write_all(b"\n");
                            let _ = si.flush();
                        }
                    }
                    // openconnect 用 --passwd-on-stdin，读到一行即可，写完关管道
                }
            }
            "SIG" => {
                if let Some(pid) = current.lock().ok().and_then(|g| *g) {
                    let hard = rest.trim() == "kill";
                    #[cfg(unix)]
                    {
                        // SAFETY: pid 有效，仅发送信号
                        unsafe {
                            libc::kill(
                                pid as libc::pid_t,
                                if hard { libc::SIGKILL } else { libc::SIGTERM },
                            );
                        }
                    }
                }
            }
            "QUIT" => break,
            _ => {}
        }
    }

    // GUI 已断开：结束 openconnect，让它调用 vpnc-script 还原路由与 DNS
    if let Some(pid) = current.lock().ok().and_then(|g| *g) {
        #[cfg(unix)]
        {
            // SAFETY: pid 有效，仅发送信号
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            let deadline = std::time::Instant::now() + Duration::from_millis(1200);
            loop {
                // 进程已退出则 kill -0 返回错误
                let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
                if !alive || std::time::Instant::now() >= deadline {
                    if alive {
                        unsafe {
                            libc::kill(pid as libc::pid_t, libc::SIGKILL);
                        }
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    std::process::exit(0);
}

// ------------------------------ GUI 侧 ------------------------------

/// 已建立的 helper 连接。
pub struct HelperLink {
    stream: TcpStream,
}

impl HelperLink {
    /// 在本机随机端口监听，然后用系统密码框以 root 启动 helper 并等待其连回。
    ///
    /// 先 bind 再启动 helper，端口由 GUI 独占，因此其他本地进程无法抢在 helper
    /// 之前冒充它连入。
    pub fn launch(exe: &std::path::Path) -> Result<Self, String> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(|e| format!("无法创建本地端口：{e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("无法读取本地端口：{e}"))?
            .port();

        // 一次性 token：经 0600 临时文件传递，helper 读完即删，不进 argv
        let token = random_token();
        let token_path = std::env::temp_dir().join(format!("octauri-helper-{token}.tok"));
        write_private(&token_path, &token)?;

        if let Err(e) = spawn_root_helper(exe, port, &token_path) {
            let _ = std::fs::remove_file(&token_path);
            return Err(e);
        }

        // 等待 helper 连回（含用户输入密码的时间）
        listener
            .set_nonblocking(false)
            .map_err(|e| format!("监听设置失败：{e}"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        let (stream, _) = loop {
            match listener.accept() {
                Ok(v) => break v,
                Err(e) => {
                    if std::time::Instant::now() >= deadline {
                        let _ = std::fs::remove_file(&token_path);
                        return Err(format!("等待辅助进程连接超时：{e}"));
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        };
        // 读 HELO 校验 token，避免误连同机其他服务
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("连接不可用：{e}"))?,
        );
        let mut helo = String::new();
        let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
        reader
            .read_line(&mut helo)
            .map_err(|e| format!("读取握手失败：{e}"))?;
        let _ = stream.set_read_timeout(None);
        let expected = format!("{HELO} {token}");
        if helo.trim_end() != expected {
            return Err("辅助进程握手校验失败（可能被其他进程占用端口）".into());
        }
        let _ = stream.set_nodelay(true);
        Ok(Self { stream })
    }

    pub fn clone_stream(&self) -> Result<TcpStream, String> {
        self.stream
            .try_clone()
            .map_err(|e| format!("helper 连接不可用：{e}"))
    }

    pub fn send(&mut self, line: &str) -> Result<(), String> {
        self.stream
            .write_all(line.as_bytes())
            .and_then(|_| self.stream.write_all(b"\n"))
            .and_then(|_| self.stream.flush())
            .map_err(|e| format!("与辅助进程通信失败：{e}"))
    }
}

/// 用系统标准密码框以 root 启动 helper。
///
/// `do shell script` 只有在输出被重定向时才会立即返回（否则会等命令结束），
/// 这里是关键：`> /dev/null 2>&1 &` 让 helper 后台运行，osascript 立刻返回。
fn spawn_root_helper(
    exe: &std::path::Path,
    port: u16,
    token_path: &std::path::Path,
) -> Result<(), String> {
    let exe = exe.to_string_lossy();
    // AppleScript 字符串用双引号包裹，内部单引号是 shell 引用
    let shell = format!(
        "'{}' --helper --connect-port {port} --helper-token-file '{}' > /dev/null 2>&1 &",
        exe.replace('\'', r#"'\''"#),
        token_path.display().to_string().replace('\'', r#"'\''"#),
    );
    let script = format!(
        "do shell script \"{}\" with prompt \"Octauri 需要管理员权限来创建 VPN 虚拟网卡。\" with administrator privileges",
        shell.replace('\\', r"\\").replace('"', r#"\""#)
    );
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("无法启动授权对话框：{e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let msg = String::from_utf8_lossy(&out.stderr);
    if msg.contains("-128") || msg.contains("User canceled") {
        Err("已取消管理员授权，无法建立连接".into())
    } else {
        Err(format!("提权失败：{}", msg.trim()))
    }
}

fn write_private(path: &std::path::Path, content: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("无法创建临时文件：{e}"))?;
    f.write_all(content.as_bytes())
        .map_err(|e| format!("无法写入临时文件：{e}"))?;
    Ok(())
}

fn random_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    // 无需密码学强度：用途只是防止误连，端口本身由 GUI 独占
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    format!("{:016x}{:016x}", h.finish(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_ascii() {
        for s in ["", "a", "ab", "abc", "openconnect --user=x"] {
            let enc = b64_encode(s.as_bytes());
            assert_eq!(b64_decode(&enc).unwrap(), s.as_bytes(), "enc={enc}");
        }
    }

    #[test]
    fn base64_roundtrip_utf8() {
        let s = "已配置为 10.255.225.16，使用 SSL 已连接";
        let enc = b64_encode(s.as_bytes());
        assert_eq!(String::from_utf8(b64_decode(&enc).unwrap()).unwrap(), s);
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
    }
}