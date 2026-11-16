//! 单实例锁：本机回环端口 + 握手协议，仅用标准库。
//!
//! 为什么不用 tauri-plugin-single-instance：它要等 Tauri 初始化完成才生效，
//! 而 macOS 上我们在初始化之前就要决定是否弹密码框提权。若用插件，
//! 已运行时再次双击仍会先弹一次密码框。这里的检测放在 `run()` 最开头，
//! 已有实例时直接唤起它的窗口并退出。
//!
//! 协议：客户端发送一行 `OCTAURI/1 SHOW`，服务端回复 `OCTAURI/1 OK` 并显示窗口。
//! 端口被其他程序占用时握手不匹配，不会误判为已有实例。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// 固定端口，仅监听 127.0.0.1，不对外暴露。
pub const PORT: u16 = 28417;

const REQ: &str = "OCTAURI/1 SHOW";
const ACK: &str = "OCTAURI/1 OK";
const TIMEOUT: Duration = Duration::from_millis(400);
/// 单行最大读取字节数，防止异常客户端持续写入
const MAX_LINE: u64 = 64;

/// 实际使用的端口。可用 `OCTAURI_PORT` 覆盖：端口被其他程序占用时用于避让，
/// 也让自动化测试不会碰到正在运行的真实实例。
pub fn port() -> u16 {
    crate::args::port().unwrap_or(PORT)
}

fn addr(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

fn read_line_limited(stream: &TcpStream) -> Option<String> {
    let mut line = String::new();
    BufReader::new(stream.take(MAX_LINE))
        .read_line(&mut line)
        .ok()?;
    Some(line.trim().to_string())
}

/// 已有实例在运行时，通知它显示窗口并返回 true。
pub fn notify_existing(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&addr(port), TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    if writeln!(stream, "{REQ}").is_err() {
        return false;
    }
    read_line_limited(&stream).as_deref() == Some(ACK)
}

/// 占用单实例端口。返回 None 表示端口已被占用。
pub fn bind(port: u16) -> Option<TcpListener> {
    TcpListener::bind(addr(port)).ok()
}

/// 后台线程接受唤起请求。每收到一次合法握手调用一次 `on_show`。
pub fn serve<F>(listener: TcpListener, on_show: F)
where
    F: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut stream) = conn else { continue };
            let _ = stream.set_read_timeout(Some(TIMEOUT));
            let _ = stream.set_write_timeout(Some(TIMEOUT));
            if read_line_limited(&stream).as_deref() != Some(REQ) {
                continue;
            }
            let _ = writeln!(stream, "{ACK}");
            on_show();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn free_port() -> u16 {
        TcpListener::bind(addr(0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn no_instance_returns_false() {
        let _g = crate::test_lock();
        assert!(!notify_existing(free_port()));
    }

    #[test]
    fn running_instance_is_notified() {
        let _g = crate::test_lock();
        let port = free_port();
        let listener = bind(port).expect("bind");
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        serve(listener, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert!(notify_existing(port));
        assert!(notify_existing(port));
        // 回调在 ACK 之后执行，留出少量时间
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn second_bind_fails() {
        let _g = crate::test_lock();
        let port = free_port();
        let _first = bind(port).expect("bind");
        assert!(bind(port).is_none());
    }

    #[test]
    fn foreign_service_is_not_mistaken_for_instance() {
        let _g = crate::test_lock();
        let port = free_port();
        let listener = TcpListener::bind(addr(port)).unwrap();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let mut s = conn;
                let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
            }
        });
        assert!(!notify_existing(port));
    }
}
