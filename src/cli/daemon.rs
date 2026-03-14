use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use crate::common::config::Config;
use crate::daemon::run_daemon;
use crate::tray::{TrayIconState, TrayState};

// ─────────────────────────────────────────────────────────────────────────────
// 公共路径辅助
// ─────────────────────────────────────────────────────────────────────────────

fn pid_path() -> Result<PathBuf> {
    Ok(Config::data_dir()?.join("daemon.pid"))
}

fn log_path() -> Result<PathBuf> {
    Ok(Config::data_dir()?.join("daemon.log"))
}

/// 读 PID 文件，返回 pid（文件不存在或内容非法返回 None）
fn read_pid() -> Option<u32> {
    let path = pid_path().ok()?;
    let s = fs::read_to_string(path).ok()?;
    s.trim().parse::<u32>().ok()
}

/// 用 kill(pid, 0) 探测进程是否存在
fn is_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// 删除 PID 文件（忽略错误）
fn remove_pid_file() {
    if let Ok(p) = pid_path() {
        let _ = fs::remove_file(p);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// start：默认后台 / 可选前台
// ─────────────────────────────────────────────────────────────────────────────

/// 后台启动：spawn 子进程运行 daemon，父进程立即退出；关掉终端不影响子进程。
pub fn start_background(model: Option<PathBuf>) -> anyhow::Result<()> {
    if let Some(pid) = read_pid() {
        if is_running(pid) {
            println!("ℹ️  Open Flow 已在运行 (PID: {})", pid);
            println!("   停止: open-flow stop");
            return Ok(());
        }
        remove_pid_file();
    }

    let exe = std::env::current_exe().context("无法获取可执行文件路径")?;
    let log = log_path()?;
    fs::create_dir_all(log.parent().unwrap())?;

    let mut args = vec!["start".to_string(), "--foreground".to_string()];
    if let Some(ref m) = model {
        args.push("--model".to_string());
        args.push(m.display().to_string());
    }

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .context("无法打开日志文件")?;

    let child = Command::new(&exe)
        .args(&args)
        .env("OPEN_FLOW_DAEMON", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .context("启动后台进程失败")?;

    let pid = child.id();
    // 不在此写 PID 文件，由子进程 start_foreground 在就绪后写入

    println!("✅ Open Flow 已在后台启动 (PID: {})", pid);
    println!("   日志: {}", log.display());
    println!("   停止: open-flow stop");
    Ok(())
}

/// 前台启动：终端被占用，Ctrl+C 或托盘「退出」可停止。
/// 主线程驱动 macOS NSRunLoop（托盘事件），tokio 跑背景线程（录音/转写/热键）。
pub fn start_foreground(model: Option<PathBuf>) -> anyhow::Result<()> {
    // ── 检查是否已在运行 ─────────────────────────────────────────────────
    if let Some(pid) = read_pid() {
        if is_running(pid) {
            println!("ℹ️  Open Flow 已在运行 (PID: {})", pid);
            println!("   停止: open-flow stop");
            return Ok(());
        }
        remove_pid_file();
    }

    // ── 临时 tokio 运行时（仅用于模型下载）────────────────────────────
    let rt_temp = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("无法创建 tokio 运行时")?;

    // ── 模型就绪（首次自动下载，异步，用 block_on 执行）──────────────
    let model_path = rt_temp
        .block_on(crate::cli::commands::setup::ensure_model_ready(model))
        .map_err(|e| {
            eprintln!("❌ 模型准备失败: {}", e);
            e
        })?;

    // ── 持久化模型路径到配置文件（方便 status 命令展示）─────────────
    if let Ok(mut config) = Config::load() {
        config.model_path = Some(model_path.clone());
        let _ = config.save();
    }

    // ── 写 PID 文件 ───────────────────────────────────────────────────
    let my_pid = std::process::id();
    fs::write(pid_path()?, my_pid.to_string())?;

    // ── 注册信号处理（SIGTERM/SIGINT → 清理 PID 并退出）─────────────
    #[cfg(unix)]
    unsafe {
        libc::signal(
            libc::SIGTERM,
            sigterm_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            sigterm_handler as *const () as libc::sighandler_t,
        );
    }

    // ── 在主线程创建托盘（macOS 要求 NSStatusItem 在主线程创建）──────
    let (tray, tray_handle) = match TrayState::new() {
        Ok((t, h)) => {
            t.set_state(TrayIconState::Idle);
            tracing::info!("✅ 托盘图标已创建");
            (Some(t), Some(Arc::new(h)))
        }
        Err(e) => {
            tracing::warn!("托盘图标创建失败: {:?}，继续运行（无托盘）", e);
            (None, None)
        }
    };

    // ── 在专用线程运行 daemon（current_thread 运行时，Daemon 含 cpal::Stream 非 Send）
    let log = log_path()?;
    println!("✅ Open Flow 已启动 (PID: {})", my_pid);
    println!("   模型: {:?}", model_path);
    println!("   热键: 右 Command（固定）");
    println!("   日志: {}", log.display());
    println!();
    println!("   按 Ctrl+C 或托盘菜单「退出」可停止");
    println!("   ⏳ 模型加载与预热约需 3-5 秒，完成后热键即可使用");

    let daemon_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("daemon tokio runtime");
        rt.block_on(async {
            if let Err(e) = run_daemon(model_path, tray_handle).await {
                eprintln!("Daemon 错误: {}", e);
            }
        });
    });

    // ── 主线程：驱动 macOS NSRunLoop，让托盘 / 菜单事件得以分发 ──────
    run_main_loop(tray.as_ref());

    // ── 退出清理 ──────────────────────────────────────────────────────
    remove_pid_file();
    // daemon 线程会在 exit_requested 后自行退出，无需 join（进程即将退出）
    let _ = daemon_handle.join();
    println!("\n👋 Open Flow 已停止");
    Ok(())
}

/// macOS 主循环：每 100ms 执行一次 NSRunLoop，检查是否需要退出。
/// 这是让 tray-icon 在 macOS 上正常渲染和响应菜单的关键。
fn run_main_loop(tray: Option<&TrayState>) {
    loop {
        // 应用 daemon 发来的托盘状态更新（灰/红/黄）
        if let Some(t) = tray {
            t.flush_state_updates();
        }

        // 驱动 macOS NSRunLoop 100ms（分发菜单/托盘事件到回调）
        #[cfg(target_os = "macos")]
        pump_run_loop_100ms();

        #[cfg(not(target_os = "macos"))]
        std::thread::sleep(std::time::Duration::from_millis(100));

        if tray.map_or(false, |t| t.exit_requested()) {
            tracing::info!("用户点击托盘退出");
            break;
        }
    }
}

/// 通过 `[NSApp nextEventMatchingMask:...]` 驱动 AppKit 事件队列。
/// NSRunLoop::runUntilDate 只处理 run loop sources，无法分发托盘点击事件；
/// 必须走 NSApplication 的事件队列才能响应 NSStatusItem 点击和菜单。
#[cfg(target_os = "macos")]
fn pump_run_loop_100ms() {
    use objc::{class, msg_send, sel, sel_impl};
    use objc::runtime::Object;

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];

        // 首次调用时设置为"配件"激活策略（无 Dock 图标，纯菜单栏 agent）
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            // NSApplicationActivationPolicyAccessory = 1
            let _: () = msg_send![app, setActivationPolicy: 1i64];
        });

        let date_cls = class!(NSDate);
        // kCFRunLoopDefaultMode
        let mode: *mut Object = msg_send![
            class!(NSString),
            stringWithUTF8String: b"kCFRunLoopDefaultMode\0".as_ptr()
                as *const std::os::raw::c_char
        ];

        // 第一次调用最多等 100ms；之后排空剩余事件（distantPast = 不阻塞）
        let deadline: *mut Object =
            msg_send![date_cls, dateWithTimeIntervalSinceNow: 0.1f64];
        let past: *mut Object = msg_send![date_cls, distantPast];

        let mut first = true;
        loop {
            let date = if first { deadline } else { past };
            first = false;

            let event: *mut Object = msg_send![
                app,
                nextEventMatchingMask: u64::MAX
                untilDate: date
                inMode: mode
                dequeue: 1u8   // YES
            ];
            if event.is_null() {
                break;
            }
            let _: () = msg_send![app, sendEvent: event];
            let _: () = msg_send![app, updateWindows];
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// stop
// ─────────────────────────────────────────────────────────────────────────────

pub async fn stop() -> Result<()> {
    let pid = match read_pid() {
        Some(p) => p,
        None => {
            println!("ℹ️  daemon 未运行（找不到 PID 文件）");
            return Ok(());
        }
    };

    if !is_running(pid) {
        println!("ℹ️  daemon 未运行（PID {} 不存在）", pid);
        remove_pid_file();
        return Ok(());
    }

    println!("⏹️  正在停止 daemon (PID: {})...", pid);
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };

    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if !is_running(pid) {
            remove_pid_file();
            println!("✅ daemon 已停止");
            return Ok(());
        }
    }

    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
    remove_pid_file();
    println!("✅ daemon 已强制终止 (SIGKILL)");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// status
// ─────────────────────────────────────────────────────────────────────────────

pub async fn status() -> Result<()> {
    let config = Config::load()?;

    match read_pid() {
        Some(pid) if is_running(pid) => {
            let uptime = get_uptime_str(pid);
            println!("Open Flow daemon 状态");
            println!("  状态:   ✅ 运行中");
            println!("  PID:    {}", pid);
            println!("  运行:   {}", uptime);
            println!("  模型:   {:?}", config.model_path.unwrap_or_default());
            println!("  热键:   右 Command（固定）");
            println!("  日志:   {}", log_path()?.display());
        }
        Some(pid) => {
            println!("Open Flow daemon 状态");
            println!("  状态:   ❌ 未运行（PID {} 已失效）", pid);
            remove_pid_file();
        }
        None => {
            println!("Open Flow daemon 状态");
            println!("  状态:   ❌ 未运行");
            println!("  启动:   open-flow start");
        }
    }
    Ok(())
}

/// SIGTERM / SIGINT 信号处理器
#[cfg(unix)]
extern "C" fn sigterm_handler(_: libc::c_int) {
    remove_pid_file();
    std::process::exit(0);
}

/// 用 ps 获取进程启动时间（仅用于展示）
fn get_uptime_str(pid: u32) -> String {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etime="])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "未知".to_string(),
    }
}
