//! 把 SIGTERM / SIGINT / SIGHUP 转为正常退出流程（仅 Unix）。
//!
//! 默认情况下这些信号会直接杀死 GUI 进程，`RunEvent::Exit` 不会触发，
//! openconnect 子进程就会以 root 孤儿进程残留（隧道、路由、DNS 都不还原）。
//! 典型来源：注销/关机、`kill <pid>`、在终端里 Ctrl+C。
//!
//! 实现：信号处理函数只往管道写 1 字节（async-signal-safe），
//! 后台线程读到后调用 `AppHandle::exit`，由正常退出路径清理子进程。

use std::sync::atomic::{AtomicI32, Ordering};
use tauri::AppHandle;

static WRITE_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn on_signal(_sig: libc::c_int) {
    let fd = WRITE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        let b: u8 = 1;
        // SAFETY: write 是 async-signal-safe；失败（管道满）可忽略，已有一个字节在途。
        unsafe {
            libc::write(fd, &b as *const u8 as *const libc::c_void, 1);
        }
    }
}

pub fn install(app: AppHandle) {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: fds 是长度为 2 的有效数组。
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        eprintln!("[octauri] 创建信号管道失败，退出时可能残留 openconnect 进程");
        return;
    }
    let (read_fd, write_fd) = (fds[0], fds[1]);
    WRITE_FD.store(write_fd, Ordering::SeqCst);

    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: 注册的处理函数只做 async-signal-safe 的 write。
        unsafe {
            libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
    }

    std::thread::spawn(move || {
        let mut b: u8 = 0;
        loop {
            // SAFETY: read_fd 由上面的 pipe 创建，仅在本线程读取。
            let n = unsafe { libc::read(read_fd, &mut b as *mut u8 as *mut libc::c_void, 1) };
            if n == 1 {
                app.exit(0);
                return;
            }
            if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
    });
}
