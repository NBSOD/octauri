//! 辅助进程协议测试（仅 macOS）。
//!
//! 直接以当前用户身份启动 `octauri --helper`，验证 GUI ↔ 辅助进程这条链路：
//! 握手 → EXEC → 日志回传 → 退出码。真实使用中该进程由系统密码框以 root 启动，
//! 但协议本身与身份无关，因此可以在测试里完整跑通。

#![cfg(target_os = "macos")]

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const HELO: &str = "OCTAURI-HELPER/1";

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn decode(arg: &str) -> String {
    const R: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let vals: Vec<u8> = arg
        .bytes()
        .filter_map(|c| R.find(c as char).map(|i| i as u8))
        .collect();
    let mut out = Vec::new();
    for c in vals.chunks(4) {
        if c.len() < 2 {
            break;
        }
        let mut n = 0u32;
        for (i, v) in c.iter().enumerate() {
            n |= u32::from(*v) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if c.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if c.len() > 3 {
            out.push(n as u8);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct Session {
    child: std::process::Child,
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stream.write_all(b"QUIT\n");
        let _ = self.stream.flush();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let _ = self.child.kill();
    }
}

fn start_helper() -> Session {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let port = listener.local_addr().unwrap().port();

    // 用例并行执行，文件名必须唯一：用端口号（每个用例各自 bind 到不同端口）
    let token = format!("testtoken{port}");
    let token_path = std::env::temp_dir().join(format!("octauri-test-{port}.tok"));
    std::fs::write(&token_path, &token).unwrap();

    let exe = env!("CARGO_BIN_EXE_octauri");
    let port_text = port.to_string();
    let child = Command::new(exe)
        .args([
            "--helper",
            "--connect-port",
            port_text.as_str(),
            "--helper-token-file",
            token_path.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("启动辅助进程失败");

    listener.set_nonblocking(false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let (stream, _) = loop {
        match listener.accept() {
            Ok(v) => break v,
            Err(_) => {
                assert!(Instant::now() < deadline, "辅助进程未在 10 秒内连回");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();

    // token 文件应被辅助进程读完即删
    let mut helo = String::new();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    reader.read_line(&mut helo).unwrap();
    assert_eq!(helo.trim_end(), format!("{HELO} {token}"));

    Session {
        child,
        stream,
        reader,
    }
}

impl Session {
    fn send(&mut self, line: &str) {
        self.stream.write_all(line.as_bytes()).unwrap();
        self.stream.write_all(b"\n").unwrap();
        self.stream.flush().unwrap();
    }

    /// 读到满足条件的一行（跳过其他事件）
    fn wait_for(&mut self, prefix: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "等待 {prefix} 超时");
            let mut line = String::new();
            let n = self.reader.read_line(&mut line).unwrap();
            assert!(n > 0, "辅助进程提前断开");
            if line.starts_with(prefix) {
                return line.trim_end().to_string();
            }
        }
    }
}

#[test]
fn helper_runs_command_and_reports_output() {
    let mut s = start_helper();
    s.send(&format!(
        "EXEC {} {} {}",
        b64(b"/bin/sh"),
        b64(b"-c"),
        b64(b"echo hello-helper")
    ));
    s.send("STDIN ");

    let pid = s.wait_for("PID ");
    assert!(pid[4..].parse::<u32>().is_ok());

    let out = s.wait_for("O out ");
    assert_eq!(decode(out.splitn(3, ' ').nth(2).unwrap()), "hello-helper");

    let exit = s.wait_for("EXIT ");
    assert_eq!(exit, "EXIT 0");
}

#[test]
fn helper_reports_nonzero_exit_code() {
    let mut s = start_helper();
    s.send(&format!(
        "EXEC {} {} {}",
        b64(b"/bin/sh"),
        b64(b"-c"),
        b64(b"exit 7")
    ));
    s.send("STDIN ");
    s.wait_for("PID ");
    assert_eq!(s.wait_for("EXIT "), "EXIT 7");
}

#[test]
fn helper_forwards_stderr_separately() {
    let mut s = start_helper();
    s.send(&format!(
        "EXEC {} {} {}",
        b64(b"/bin/sh"),
        b64(b"-c"),
        b64(b"echo oops 1>&2")
    ));
    s.send("STDIN ");
    s.wait_for("PID ");
    let err = s.wait_for("O err ");
    assert_eq!(decode(err.splitn(3, ' ').nth(2).unwrap()), "oops");
}

#[test]
fn helper_reports_spawn_failure() {
    let mut s = start_helper();
    s.send(&format!("EXEC {}", b64(b"/nonexistent/octauri-test-bin")));
    s.send("STDIN ");
    let err = s.wait_for("ERR ");
    assert!(!decode(err.splitn(2, ' ').nth(1).unwrap()).is_empty());
}

#[test]
fn helper_terminates_child_on_quit() {
    let mut s = start_helper();
    s.send(&format!(
        "EXEC {} {} {}",
        b64(b"/bin/sh"),
        b64(b"-c"),
        // 忽略 TERM 的长期进程，验证升级到 SIGKILL
        b64(b"trap '' TERM; echo started; while :; do sleep 0.1; done")
    ));
    s.send("STDIN ");
    let pid: u32 = s.wait_for("PID ")[4..].parse().unwrap();
    s.wait_for("O out ");

    s.send("QUIT");
    drop_quit(&mut s);

    // 辅助进程退出后，子进程必须已被清理
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // SAFETY: 仅探测进程是否存在
        let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        if !alive {
            break;
        }
        assert!(Instant::now() < deadline, "子进程 {pid} 未被清理");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 发完 QUIT 后只等辅助进程自身退出，不再依赖 Drop 的收尾
fn drop_quit(s: &mut Session) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        if matches!(s.child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("辅助进程未在收到 QUIT 后退出");
}