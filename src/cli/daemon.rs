use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::info;

use crate::common::config::Config;
use crate::daemon::run_daemon;

// ── 内部环境变量，标识"我是被父进程 spawn 出来的后台实例" ──────────────
const DAEMON_INTERNAL_ENV: &str = "OPEN_FLOW_DAEMON_INTERNAL";

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
// start
// ─────────────────────────────────────────────────────────────────────────────

pub async fn start(model: Option<PathBuf>, hotkey: String) -> Result<()> {
    // ── 如果是被父进程重新 spawn 出来的，直接运行 daemon 主循环 ──────────
    if std::env::var(DAEMON_INTERNAL_ENV).is_ok() {
        return run_daemon_foreground(model, hotkey).await;
    }

    // ── 检查是否已在运行 ─────────────────────────────────────────────────
    if let Some(pid) = read_pid() {
        if is_running(pid) {
            println!("ℹ️  Open Flow daemon 已在运行 (PID: {})", pid);
            println!("   日志: {}", log_path()?.display());
            println!("   停止: open-flow stop");
            return Ok(());
        }
        // 僵尸 PID 文件，清理
        remove_pid_file();
    }

    // ── 准备日志文件（追加模式） ─────────────────────────────────────────
    let log = log_path()?;
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .with_context(|| format!("无法创建日志文件: {}", log.display()))?;
    let log_file2 = log_file.try_clone()?;

    // ── spawn 自身，传入内部标记 ─────────────────────────────────────────
    let exe = std::env::current_exe().context("无法获取当前可执行文件路径")?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("start");
    if let Some(ref m) = model {
        cmd.arg("--model").arg(m);
    }
    cmd.arg("--hotkey").arg(&hotkey);
    cmd.env(DAEMON_INTERNAL_ENV, "1")
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_file2);

    // setsid：让子进程脱离当前终端会话，关掉终端不会收到 SIGHUP
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let child = cmd.spawn().context("启动后台 daemon 失败")?;
    let pid = child.id();

    // 写 PID 文件（父进程写，子进程的 PID 在 spawn 后立即已知）
    fs::write(pid_path()?, pid.to_string())?;

    // 父进程不 wait()，让子进程独立运行
    std::mem::forget(child);

    println!("✅ Open Flow daemon 已在后台启动");
    println!("   PID:  {}", pid);
    println!("   日志: {}", log.display());
    println!("   停止: open-flow stop");
    println!("   状态: open-flow status");

    Ok(())
}

/// 真正的 daemon 主循环（在后台子进程中调用）
async fn run_daemon_foreground(model: Option<PathBuf>, hotkey: String) -> Result<()> {
    // 重写 PID 文件（子进程确认自己的 PID，防止父进程 PID 与子进程不同）
    let my_pid = std::process::id();
    if let Ok(p) = pid_path() {
        let _ = fs::write(&p, my_pid.to_string());
    }

    // 注册 SIGTERM 处理：收到信号时清理 PID 文件后退出
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as libc::sighandler_t);
        libc::signal(libc::SIGINT, sigterm_handler as libc::sighandler_t);
    }

    // 加载配置
    let mut config = Config::load().context("加载配置失败")?;
    if let Some(m) = model {
        config.model_path = Some(m);
    }
    config.hotkey = hotkey;
    config.save().context("保存配置失败")?;

    let model_path = config
        .model_path
        .clone()
        .context("未配置模型路径。请先运行: open-flow config set-model <path>")?;

    if !model_path.exists() {
        anyhow::bail!("模型路径不存在: {:?}", model_path);
    }

    println!("🚀 Open Flow daemon 启动中 (PID: {})...", my_pid);
    println!("   模型: {:?}", model_path);
    println!("   热键: {}", config.hotkey);

    // 运行 daemon（阻塞直到退出）
    let result = run_daemon(config, model_path).await;

    // 退出时清理 PID 文件
    remove_pid_file();
    result
}

/// SIGTERM / SIGINT 信号处理器（unsafe C 风格）
#[cfg(unix)]
extern "C" fn sigterm_handler(_: libc::c_int) {
    remove_pid_file();
    std::process::exit(0);
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

    // 发 SIGTERM，等最多 3 秒，再 SIGKILL
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

    // 超时：强制 SIGKILL
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
            // 从 /proc 或 ps 读取进程启动时间（macOS 用 ps）
            let uptime = get_uptime_str(pid);
            println!("Open Flow daemon 状态");
            println!("  状态:   ✅ 运行中");
            println!("  PID:    {}", pid);
            println!("  运行:   {}", uptime);
            println!("  模型:   {:?}", config.model_path.unwrap_or_default());
            println!("  热键:   {}", config.hotkey);
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

/// 用 ps 获取进程启动时间（仅用于展示）
fn get_uptime_str(pid: u32) -> String {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "etime="])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "未知".to_string(),
    }
}
