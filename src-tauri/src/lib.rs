//! Octauri —— openconnect 命令行的图形外壳。
//!
//! ⚠️ 本程序不实现任何 VPN / DTLS / TLS 协议栈，全部隧道能力由外部
//! openconnect 可执行文件提供，本程序只负责：配置管理、子进程拉起、
//! 日志采集、状态呈现与进程终止。

mod args;
mod clipboard;
mod commands;
#[cfg(target_os = "macos")]
mod helper;
#[cfg(target_os = "macos")]
mod macos_quit;
mod model;
mod prefs;
mod probe;
mod runner;
#[cfg(unix)]
mod signals;
mod single;
mod store;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};

use commands::AppState;

/// 测试专用：串行化「fork 子进程」与「绑定端口」的测试。
/// macOS 上 socket 创建与设置 CLOEXEC 之间有窗口期，并发 fork 会让子进程继承端口。
#[cfg(test)]
pub(crate) static TEST_PROC_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_PROC_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
use runner::Runner;

/// 以 root 运行时，Tauri 默认会把数据写到 /var/root（用户看不到、也无法备份）。
/// 这里把数据目录重定向到真实登录用户的 Library 下。
#[cfg(target_os = "macos")]
fn user_data_dir_fallback() -> Option<std::path::PathBuf> {
    use std::process::{Command, Stdio};
    // /dev/console 的所有者就是当前登录的桌面用户
    let out = Command::new("stat")
        .args(["-f", "%Su", "/dev/console"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let user = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if user.is_empty() || user == "root" || user == "loginwindow" {
        return None;
    }
    let home = std::path::PathBuf::from("/Users").join(&user);
    if !home.is_dir() {
        return None;
    }
    Some(
        home.join("Library/Application Support")
            .join("io.github.nbsod.octauri"),
    )
}

fn resolve_data_dir(app: &tauri::App) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        // SAFETY: geteuid 无参数、无副作用。
        if unsafe { libc::geteuid() } == 0 {
            if let Some(dir) = user_data_dir_fallback() {
                return dir;
            }
        }
    }
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// macOS 菜单栏：Tauri 默认菜单是英文，这里自建简体中文版本。
/// 「编辑」菜单同时承担 Cmd+C / Cmd+V 等快捷键（macOS 的复制快捷键由菜单驱动）。
#[cfg(target_os = "macos")]
fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    use tauri::menu::Submenu;
    let app_menu = Submenu::with_items(
        app,
        "Octauri",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("关于 Octauri"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, Some("服务"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("隐藏 Octauri"))?,
            &PredefinedMenuItem::hide_others(app, Some("隐藏其他"))?,
            &PredefinedMenuItem::show_all(app, Some("全部显示"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("退出 Octauri"))?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "编辑",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("撤销"))?,
            &PredefinedMenuItem::redo(app, Some("重做"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("剪切"))?,
            &PredefinedMenuItem::copy(app, Some("拷贝"))?,
            &PredefinedMenuItem::paste(app, Some("粘贴"))?,
            &PredefinedMenuItem::select_all(app, Some("全选"))?,
        ],
    )?;
    let window_menu = Submenu::with_items(
        app,
        "窗口",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("最小化"))?,
            &PredefinedMenuItem::fullscreen(app, Some("进入全屏幕"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("关闭窗口"))?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu])
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let connect = MenuItem::with_id(app, "connect", "连接", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "断开", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "打开窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &connect,
            &disconnect,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        // 左键单击直接唤起窗口，右键才出菜单
        .show_menu_on_left_click(false)
        .tooltip("Octauri")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "connect" => {
                // 交给前端处理（可能需要输入密码）
                let _ = app.emit("oc://tray-connect", ());
                show_main(app);
            }
            "disconnect" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = state.runner.stop(app, false);
                }
            }
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });

    if let Ok(img) = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
        builder = builder.icon(img);
    }
    #[cfg(target_os = "macos")]
    {
        // macOS 模板图标随主题自动反色
        builder = builder.icon_as_template(true);
    }
    builder.build(app)?;
    Ok(())
}

fn setup_window(app: &tauri::App) {
    if let Some(win) = app.get_webview_window("main") {
        let w = win.clone();
        win.on_window_event(move |event| {
            // 关闭窗口 = 收起到托盘，保持连接不中断
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = w.hide();
            }
        });
    }
}

pub fn run() {
    // 0) 辅助进程模式：以 root 运行，只服务 openconnect 的拉起与 I/O 转发，
    //    不初始化 Tauri。必须在单实例检测之前分流，否则会与 GUI 抢端口。
    #[cfg(target_os = "macos")]
    if let Some(port) = args::helper_port() {
        helper::helper_main(port, args::helper_token_file());
    }

    // 1) 单实例：已有实例时唤起其窗口后直接退出。
    //    放在提权之前，避免已运行时再次双击还弹一次密码框。
    if single::notify_existing(single::port()) {
        std::process::exit(0);
    }

    // 2) 占用单实例端口。被其他程序占用时仍可启动，只是没有单实例保护
    let mut instance_listener = single::bind(single::port());
    if instance_listener.is_none() {
        eprintln!(
            "[octauri] 单实例端口 127.0.0.1:{} 被占用，本次运行不启用单实例保护",
            single::port()
        );
    }

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(build_app_menu);

    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            if let Some(listener) = instance_listener.take() {
                let handle = app.handle().clone();
                single::serve(listener, move || show_main(&handle));
            }
            let dir = resolve_data_dir(app);
            let _ = std::fs::create_dir_all(&dir);
            app.manage(AppState {
                store: store::Store::new(dir.clone()),
                runner: Runner::new(),
                prefs: prefs::Prefs::load(dir),
            });
            setup_tray(app)?;
            setup_window(app);
            #[cfg(unix)]
            signals::install(app.handle().clone());
            // macOS：后台启动 root 辅助进程（弹一次系统密码框）。
            // GUI 自身保持普通用户身份，openconnect 交给辅助进程拉起。
            #[cfg(target_os = "macos")]
            if !args::no_elevate() {
                if let Some(state) = app.try_state::<AppState>() {
                    let runner = state.runner.clone();
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        let Ok(exe) = std::env::current_exe() else {
                            let _ = handle.emit("oc://helper-error", "无法定位程序路径");
                            return;
                        };
                        match helper::HelperLink::launch(&exe) {
                            Ok(link) => match runner.attach_helper(handle.clone(), link) {
                                Ok(()) => {
                                    let _ = handle.emit("oc://helper-ready", ());
                                }
                                Err(e) => {
                                    let _ = handle.emit("oc://helper-error", e);
                                }
                            },
                            Err(e) => {
                                let _ = handle.emit("oc://helper-error", e);
                            }
                        }
                    });
                }
            }
            // 接管 Dock 退出与 Cmd+Q（都走 NSApplication terminate:）
            #[cfg(target_os = "macos")]
            macos_quit::install(app.handle().clone());
            #[cfg(all(target_os = "macos", debug_assertions))]
            if let Some(ms) = args::terminate_after_ms() {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    crate::macos_quit::self_terminate();
                });
            }
            // 自测钩子（仅 debug 构建）：用指定可执行文件起一个连接。
            // 放到后台并等待 root 辅助进程就绪，以便验证 helper 链路。
            #[cfg(debug_assertions)]
            if let Some(bin) = args::spawn() {
                if let Some(state) = app.try_state::<AppState>() {
                    let runner = state.runner.clone();
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        for _ in 0..200 {
                            if runner.has_helper() || args::no_elevate() {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        let profile = crate::model::Profile {
                            id: "selftest".into(),
                            name: "selftest".into(),
                            gateway: "selftest.invalid".into(),
                            username: "tester".into(),
                            auth_method: crate::model::AuthMethod::Password,
                            protocol: String::new(),
                            certificate_path: String::new(),
                            key_path: String::new(),
                            extra_args: "--scenario=hang".into(),
                            openconnect_path: bin.clone(),
                            servercert: String::new(),
                            created_at: 0,
                            updated_at: 0,
                        };
                        match runner.start(&handle, &profile, std::path::Path::new(&bin), Some("x".into())) {
                            Ok(pid) => eprintln!("[octauri] self-test: spawned child pid={pid}"),
                            Err(e) => eprintln!("[octauri] self-test: spawn failed: {e}"),
                        }
                    });
                }
            }
            // 自测钩子（仅 debug 构建）：N 毫秒后走 app.exit(0)，用于验证退出路径
            #[cfg(debug_assertions)]
            if let Some(ms) = args::exit_after_ms() {
                let h = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    eprintln!("[octauri] self-test: app.exit(0) at {ms}ms");
                    h.exit(0);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::profiles_list,
            commands::profile_save,
            commands::profile_delete,
            commands::profile_import,
            commands::profile_export,
            commands::env_probe,
            commands::connect,
            commands::disconnect,
            commands::force_kill,
            commands::conn_state,
            commands::helper_status,
            commands::data_dir_status,
            commands::logs_tail,
            commands::logs_clear,
            commands::logs_export,
            prefs::prefs_get,
            prefs::prefs_set,
            clipboard::clipboard_write,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build tauri application");

    app.run(|app_handle, event| {
        // 所有退出路径（托盘「退出」、Cmd+Q、菜单「退出 Octauri」）最终都会到这里：
        // 先结束 openconnect 子进程，否则它会以 root 孤儿进程残留，隧道也不会断开
        if let RunEvent::Exit = event {
            if let Some(state) = app_handle.try_state::<AppState>() {
                state.runner.shutdown();
            }
            // 清理完成后直接结束进程。
            //
            // 为什么不用 app.exit() 的自然收尾：macOS 上 tao 靠 NSApp stop 退出
            // run loop，而 stop 需要一个事件才能唤醒。Exit 回调里等待子进程会占用
            // 主线程，收尾可能因此不完成——现象是端口已释放、openconnect 已结束，
            // 但 GUI 进程仍挂着（用户实测遇到）。此处所有需要落盘的数据在改动时
            // 已写入，直接结束是安全的。
            std::process::exit(0);
        }
        // macOS 点击 Dock 图标时重新显示窗口
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen {
            has_visible_windows,
            ..
        } = event
        {
            if !has_visible_windows {
                show_main(app_handle);
            }
        }
    });
}

/// 让 Arc<Runner> 在跨线程共享时保持可读性
#[allow(dead_code)]
type SharedRunner = Arc<Runner>;