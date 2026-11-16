//! 接管 macOS 的退出请求（仅 macOS）。
//!
//! 为什么需要：Dock 图标右键「退出」、Cmd+Q、系统注销/关机都会触发 AppKit 的终止流程，
//! 而该流程会尝试关闭所有窗口。tao 的 `windowShouldClose:` 恒定返回 NO，
//! 由应用层决定是否关闭；我们为了「关窗口收起到托盘」调用了 prevent_close，
//! 窗口因此不会关闭，终止就被取消了 —— 表现为点退出后进程仍在，单实例端口还占着。
//!
//! 两个入口都要接管（实测只挂 `terminate:` 不够，Dock 退出不经过它）：
//! 1. `-[NSApplication terminate:]`：Cmd+Q 与菜单「退出」走这里；
//! 2. delegate 的 `applicationShouldTerminate:`：Dock 退出与系统关机的 Apple Event 走这里。
//!    tao 的 delegate 未实现该方法，因此动态补上；返回 NSTerminateCancel 阻止 AppKit
//!    继续关窗口，同时由我们走 `AppHandle::exit`（托盘「退出」那条已验证的路径）。
//!
//! 不改 delegate 实例、不引入新依赖，只用 Objective-C 运行时 C 接口。
//!
//! 说明：`terminate:` 接管已实测有效（内部调用 [NSApp terminate:] 后日志出现
//! `terminate: intercepted`，进程按预期退出）。`applicationShouldTerminate:`
//! 无法在本地自动化验证 —— 外部发起的 quit Apple Event 需要「自动化」权限，
//! 未授权时会被系统拦掉（-10004）。真实 Dock 退出由系统内部投递，不受此限制。

use std::ffi::{c_char, c_void};
use std::io::Write;
use std::sync::OnceLock;
use tauri::AppHandle;

static APP: OnceLock<AppHandle> = OnceLock::new();
/// terminate: 的原始实现，仅在拿不到 AppHandle 时兜底
static ORIG: OnceLock<usize> = OnceLock::new();

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn class_getInstanceMethod(cls: *mut c_void, sel: *mut c_void) -> *mut c_void;
    fn class_addMethod(
        cls: *mut c_void,
        sel: *mut c_void,
        imp: usize,
        types: *const c_char,
    ) -> bool;
    fn method_getImplementation(method: *mut c_void) -> usize;
    fn method_setImplementation(method: *mut c_void, imp: usize) -> usize;
    fn object_getClass(obj: *mut c_void) -> *mut c_void;
    fn objc_msgSend();
}

type TerminateFn = extern "C" fn(*mut c_void, *mut c_void);

#[cfg(debug_assertions)]
fn dbg(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/octauri-quit.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}
#[cfg(not(debug_assertions))]
fn dbg(_msg: &str) {}

/// 走正常退出：触发 RunEvent::Exit → 结束 openconnect → 进程退出
fn request_quit() -> bool {
    match APP.get() {
        Some(app) => {
            app.exit(0);
            true
        }
        None => false,
    }
}

extern "C" fn on_terminate(obj: *mut c_void, cmd: *mut c_void) {
    dbg("terminate: intercepted");
    if request_quit() {
        return;
    }
    // 理论上不会发生（install 先于任何退出请求）；兜底交给原实现
    if let Some(orig) = ORIG.get() {
        // SAFETY: orig 来自 method_getImplementation，签名与 terminate: 一致
        let f: TerminateFn = unsafe { std::mem::transmute(*orig) };
        f(obj, cmd);
    }
}

/// NSApplicationTerminateReply: 0 = NSTerminateCancel，1 = NSTerminateNow
extern "C" fn on_should_terminate(
    _obj: *mut c_void,
    _cmd: *mut c_void,
    _sender: *mut c_void,
) -> usize {
    dbg("applicationShouldTerminate: intercepted");
    if request_quit() {
        dbg("applicationShouldTerminate: -> cancel, quitting via app.exit");
        // 阻止 AppKit 继续关闭窗口，由我们自己完成退出
        return 0;
    }
    1
}

/// 自测：从内部调用 [NSApp terminate:]，用于验证接管是否生效
#[allow(dead_code)]
pub fn self_terminate() {
    // SAFETY: 标准 objc_msgSend 调用，类与方法均存在
    unsafe {
        let cls = objc_getClass(c"NSApplication".as_ptr());
        let shared = sel_registerName(c"sharedApplication".as_ptr());
        let get_app: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let ns_app = get_app(cls, shared);
        if ns_app.is_null() {
            return;
        }
        let sel = sel_registerName(c"terminate:".as_ptr());
        let send: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
            std::mem::transmute(objc_msgSend as *const ());
        dbg("calling [NSApp terminate:]");
        send(ns_app, sel, std::ptr::null_mut());
    }
}

pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    // SAFETY: 仅操作 Objective-C 运行时；类名/选择器固定存在，指针非空即有效。
    unsafe {
        let cls = objc_getClass(c"NSApplication".as_ptr());
        if cls.is_null() {
            eprintln!("[octauri] 未找到 NSApplication，退出时可能残留进程");
            return;
        }

        // 1) Cmd+Q / 菜单「退出」
        let sel = sel_registerName(c"terminate:".as_ptr());
        let method = class_getInstanceMethod(cls, sel);
        if method.is_null() {
            eprintln!("[octauri] 未找到 terminate:，Cmd+Q 退出可能残留进程");
        } else {
            let _ = ORIG.set(method_getImplementation(method));
            method_setImplementation(method, on_terminate as *const () as usize);
        }

        // 2) Dock 退出 / 系统关机：给当前 delegate 的类补上 applicationShouldTerminate:
        let shared = sel_registerName(c"sharedApplication".as_ptr());
        let get_app: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let ns_app = get_app(cls, shared);
        if ns_app.is_null() {
            eprintln!("[octauri] 未取到 sharedApplication，Dock 退出可能残留进程");
            return;
        }
        let delegate_sel = sel_registerName(c"delegate".as_ptr());
        let get_delegate: extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const ());
        let delegate = get_delegate(ns_app, delegate_sel);
        if delegate.is_null() {
            eprintln!("[octauri] 无 delegate，Dock 退出可能残留进程");
            return;
        }
        let delegate_cls = object_getClass(delegate);
        let should_sel = sel_registerName(c"applicationShouldTerminate:".as_ptr());
        let added = class_addMethod(
            delegate_cls,
            should_sel,
            on_should_terminate as *const () as usize,
            c"Q@:@".as_ptr(),
        );
        dbg(&format!("hook installed: terminate_ok={} should_terminate_added={added}", method.is_null() == false));
        if !added {
            // 已有实现则覆盖，确保我们的逻辑生效
            let m = class_getInstanceMethod(delegate_cls, should_sel);
            if !m.is_null() {
                method_setImplementation(m, on_should_terminate as *const () as usize);
            } else {
                eprintln!("[octauri] 未能接管 applicationShouldTerminate:，Dock 退出可能残留进程");
            }
        }
    }
}