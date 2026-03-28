use anyhow::{Context, Result};
use open_flow::model_store;
use serde::Serialize;
use std::fs;
#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::asr::groq::GroqAsrProvider;
use crate::asr::{AsrProvider, LocalAsrProvider};
use crate::common::config::Config;
use crate::common::ui::UiLanguage;
use crate::daemon::run_daemon;
use crate::tray::{TrayIconState, TrayState};

/// SIGTERM/SIGINT 收到后设为 true，由主循环检测后正常退出
static SIGNAL_SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
const GITHUB_REPO: &str = "jqlong17/open-flow";

#[cfg(target_os = "macos")]
fn log_update_info(message: impl AsRef<str>) {
    eprintln!("[Updater] {}", message.as_ref());
}

#[cfg(target_os = "macos")]
fn log_update_error(context: &str, err: &anyhow::Error) {
    eprintln!("[Updater] {}: {}", context, err);
    for (idx, cause) in err.chain().enumerate().skip(1) {
        eprintln!("[Updater]   cause[{idx}]: {}", cause);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 公共路径辅助
// ─────────────────────────────────────────────────────────────────────────────

fn pid_path() -> Result<PathBuf> {
    Ok(Config::data_dir()?.join("daemon.pid"))
}

fn log_path() -> Result<PathBuf> {
    Ok(Config::data_dir()?.join("daemon.log"))
}

#[cfg(target_os = "macos")]
fn running_app_bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|p| p.extension().and_then(|s| s.to_str()) == Some("app"))
        .map(|p| p.to_path_buf())
}

#[cfg(target_os = "macos")]
fn shell_single_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[cfg(target_os = "macos")]
#[derive(serde::Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[cfg(target_os = "macos")]
fn fetch_latest_macos_app_asset() -> Result<(String, String)> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("无法创建更新检查运行时")?;

    let release: GithubRelease = rt.block_on(async {
        let client = reqwest::Client::builder()
            .user_agent("open-flow-updater")
            .build()
            .context("创建 HTTP 客户端失败")?;
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_REPO
        );
        log_update_info(format!("检查最新 release: {}", url));
        let resp = client
            .get(url)
            .send()
            .await
            .context("请求 latest release 失败")?
            .error_for_status()
            .context("latest release 返回错误状态")?;
        let body = resp.text().await.context("读取 latest release 响应失败")?;
        let parsed = serde_json::from_str::<GithubRelease>(&body)
            .context("解析 latest release JSON 失败")?;
        Ok::<_, anyhow::Error>(parsed)
    })?;

    let arch_asset_suffix = if cfg!(target_arch = "aarch64") {
        "macos-aarch64.app.zip"
    } else {
        "macos-x86_64.app.zip"
    };

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(arch_asset_suffix))
        .or_else(|| release.assets.iter().find(|a| a.name.ends_with(".app.zip")))
        .ok_or_else(|| anyhow::anyhow!("latest release 未找到 macOS .app.zip 资产"))?;

    log_update_info(format!(
        "最新版本 {}，选中更新资产 {} -> {}",
        release.tag_name, asset.name, asset.browser_download_url
    ));

    Ok((asset.browser_download_url.clone(), release.tag_name))
}

#[cfg(target_os = "macos")]
fn download_latest_app_zip<F>(download_url: &str, tag: &str, mut on_progress: F) -> Result<PathBuf>
where
    F: FnMut(u64, Option<u64>),
{
    let update_dir = Config::data_dir()?.join("updates");
    fs::create_dir_all(&update_dir)?;

    let zip_path = update_dir.join(format!(
        "open-flow-{}-{}.app.zip",
        tag.trim_start_matches('v'),
        uuid::Uuid::new_v4()
    ));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("无法创建下载运行时")?;

    log_update_info(format!(
        "开始下载更新包 tag={} url={} path={}",
        tag,
        download_url,
        zip_path.display()
    ));

    let download_result = rt.block_on(async {
        use tokio::io::AsyncWriteExt;

        let client = reqwest::Client::builder()
            .user_agent("open-flow-updater")
            .build()
            .context("创建下载客户端失败")?;
        let mut resp = client
            .get(download_url)
            .send()
            .await
            .with_context(|| format!("下载更新包失败: {}", download_url))?
            .error_for_status()
            .with_context(|| format!("更新包下载返回错误状态: {}", download_url))?;

        let total = resp.content_length();
        let mut file = tokio::fs::File::create(&zip_path)
            .await
            .with_context(|| format!("创建更新包文件失败: {}", zip_path.display()))?;
        let mut downloaded: u64 = 0;

        while let Some(chunk) = resp
            .chunk()
            .await
            .with_context(|| format!("读取下载分块失败: {}", download_url))?
        {
            file.write_all(&chunk)
                .await
                .with_context(|| format!("写入更新包失败: {}", zip_path.display()))?;
            downloaded += chunk.len() as u64;
            on_progress(downloaded, total);
        }

        file.flush().await.context("刷新更新包文件失败")?;
        log_update_info(format!(
            "更新包下载完成 tag={} bytes={} path={}",
            tag,
            downloaded,
            zip_path.display()
        ));
        Ok::<_, anyhow::Error>(())
    });

    if let Err(err) = download_result {
        let _ = fs::remove_file(&zip_path);
        return Err(err);
    }

    Ok(zip_path)
}

#[cfg(target_os = "macos")]
fn launch_app_replacement_worker(
    app_bundle: &Path,
    zip_path: &Path,
    current_pid: u32,
) -> Result<()> {
    let update_dir = Config::data_dir()?.join("updates");
    fs::create_dir_all(&update_dir)?;
    let script_path = update_dir.join(format!("open-flow-updater-{}.sh", uuid::Uuid::new_v4()));

    let app_q = shell_single_quote(app_bundle);
    let zip_q = shell_single_quote(zip_path);

    let script = format!(
        r#"#!/bin/bash
set -euo pipefail
set -x
PID={pid}
ZIP={zip}
APP_PATH={app}
TMP_DIR=$(/usr/bin/mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

echo "[Updater] worker start PID=$PID ZIP=$ZIP APP_PATH=$APP_PATH"

for _ in $(seq 1 60); do
  if ! /bin/kill -0 "$PID" 2>/dev/null; then
    break
  fi
  /bin/sleep 1
done

/usr/bin/ditto -x -k "$ZIP" "$TMP_DIR"
NEW_APP="$TMP_DIR/Open Flow.app"
if [ ! -d "$NEW_APP" ]; then
  echo "Update package missing Open Flow.app"
  exit 1
fi

install_cmd() {{
  /bin/rm -rf "$APP_PATH" && /usr/bin/ditto "$NEW_APP" "$APP_PATH"
}}

if ! install_cmd; then
  CMD="/bin/rm -rf \"$APP_PATH\" && /usr/bin/ditto \"$NEW_APP\" \"$APP_PATH\""
  ESCAPED=${{CMD//\\/\\\\}}
  ESCAPED=${{ESCAPED//\"/\\\"}}
  /usr/bin/osascript -e "do shell script \"$ESCAPED\" with administrator privileges"
fi

if [ ! -d "$APP_PATH" ]; then
  echo "App install failed: $APP_PATH"
  exit 1
fi

/usr/bin/open "$APP_PATH"
"#,
        pid = current_pid,
        zip = zip_q,
        app = app_q,
    );

    log_update_info(format!(
        "启动安装脚本 pid={} app={} zip={} script={}",
        current_pid,
        app_bundle.display(),
        zip_path.display(),
        script_path.display()
    ));

    let mut file = fs::File::create(&script_path)
        .with_context(|| format!("无法创建更新脚本: {}", script_path.display()))?;
    file.write_all(script.as_bytes())?;
    drop(file);

    let mut perms = fs::metadata(&script_path)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
    }
    fs::set_permissions(&script_path, perms)?;

    let updater_log = update_dir.join("updater.log");
    let updater_log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&updater_log)
        .with_context(|| format!("无法打开升级日志: {}", updater_log.display()))?;

    Command::new("/bin/bash")
        .arg(script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(updater_log_file.try_clone()?))
        .stderr(Stdio::from(updater_log_file))
        .spawn()
        .context("启动升级器失败")?;

    Ok(())
}

#[cfg(target_os = "macos")]
enum UpdateDownloadResult {
    UpToDate {
        latest_tag: String,
    },
    ReadyToInstall {
        zip_path: PathBuf,
        latest_tag: String,
    },
}

#[cfg(target_os = "macos")]
enum UpdateDownloadEvent {
    Progress { percent: u8 },
    Completed(Result<UpdateDownloadResult, String>),
}

#[cfg(target_os = "macos")]
fn check_and_download_app_update<F>(mut on_progress: F) -> Result<UpdateDownloadResult>
where
    F: FnMut(u64, Option<u64>),
{
    let ui = Config::load()
        .map(|config| UiLanguage::from_config(&config))
        .unwrap_or_default();
    let (download_url, latest_tag) = fetch_latest_macos_app_asset()?;
    let current_tag = format!("v{}", env!("CARGO_PKG_VERSION"));
    log_update_info(format!("当前版本 {}，最新版本 {}", current_tag, latest_tag));
    if latest_tag == current_tag {
        return Ok(UpdateDownloadResult::UpToDate { latest_tag });
    }

    println!(
        "{}",
        ui.pick(
            format!("⬇️  检测到新版本 {}，正在下载更新包...", latest_tag),
            format!(
                "⬇️  New version {} found. Downloading update package...",
                latest_tag
            ),
        )
    );
    let zip_path = download_latest_app_zip(&download_url, &latest_tag, |downloaded, total| {
        on_progress(downloaded, total)
    })?;
    Ok(UpdateDownloadResult::ReadyToInstall {
        zip_path,
        latest_tag,
    })
}

#[cfg(target_os = "macos")]
fn start_install_downloaded_app_update(zip_path: &Path, current_pid: u32) -> Result<()> {
    let ui = Config::load()
        .map(|config| UiLanguage::from_config(&config))
        .unwrap_or_default();
    let app_bundle = running_app_bundle_path().ok_or_else(|| {
        anyhow::anyhow!(
            "{}",
            ui.pick(
                "当前不是 .app 内运行。请从 /Applications/Open Flow.app 启动后再使用菜单升级。",
                "Open Flow is not running from an .app bundle. Please launch /Applications/Open Flow.app before updating from the menu."
            )
        )
    })?;

    log_update_info(format!(
        "准备安装已下载更新 zip={} app={} pid={}",
        zip_path.display(),
        app_bundle.display(),
        current_pid
    ));
    launch_app_replacement_worker(&app_bundle, zip_path, current_pid)?;
    println!(
        "{}",
        ui.pick(
            "🚀 升级器已启动，Open Flow 即将退出并安装新版本",
            "🚀 Updater launched. Open Flow will quit and install the new version."
        )
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn show_update_popup(ui: UiLanguage, message: &str) {
    let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        "display dialog \"{}\" buttons {{{}}} default button {} with title {}",
        escaped,
        ui.pick("\"好\"", "\"OK\""),
        ui.pick("\"好\"", "\"OK\""),
        ui.pick("\"Open Flow 更新\"", "\"Open Flow Update\"")
    );
    let _ = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .spawn();
}

/// 读 PID 文件，返回 pid（文件不存在或内容非法返回 None）
fn read_pid() -> Option<u32> {
    let path = pid_path().ok()?;
    let s = fs::read_to_string(path).ok()?;
    s.trim().parse::<u32>().ok()
}

/// 探测进程是否存在（Unix: kill(pid,0)；Windows: OpenProcess + GetExitCodeProcess）
fn is_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            return false;
        }
        // 验证是否确实是 open-flow 进程，而不是被回收的 PID
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
        {
            let comm = String::from_utf8_lossy(&output.stdout);
            return comm.trim().contains("open-flow");
        }
        true // ps 失败则假定是我们的进程
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        const STILL_ACTIVE: u32 = 259;
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h == 0 || h == -1_i32 as isize {
            return false;
        }
        let mut code: u32 = 0;
        let ok = unsafe { GetExitCodeProcess(h as HANDLE, &mut code) != 0 };
        unsafe { CloseHandle(h as HANDLE) };
        ok && code == STILL_ACTIVE
    }
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
    let ui = Config::load()
        .map(|config| UiLanguage::from_config(&config))
        .unwrap_or_default();
    if let Some(pid) = read_pid() {
        if is_running(pid) {
            println!(
                "{}",
                ui.pick(
                    format!("ℹ️  Open Flow 已在运行 (PID: {})", pid),
                    format!("ℹ️  Open Flow is already running (PID: {})", pid),
                )
            );
            println!(
                "   {}",
                ui.pick("停止: open-flow stop", "Stop: open-flow stop")
            );
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
    // 父进程立即写 PID 文件，stop 命令无需等子进程就绪
    fs::write(pid_path()?, pid.to_string()).context("写入 PID 文件失败")?;

    println!(
        "{}",
        ui.pick(
            format!("✅ Open Flow 已在后台启动 (PID: {})", pid),
            format!("✅ Open Flow started in the background (PID: {})", pid),
        )
    );
    println!("   {} {}", ui.pick("日志:", "Log:"), log.display());
    println!(
        "   {}",
        ui.pick("停止: open-flow stop", "Stop: open-flow stop")
    );
    Ok(())
}

/// 前台启动：终端被占用，Ctrl+C 或托盘「退出」可停止。
/// 主线程驱动 macOS NSRunLoop（托盘事件），tokio 跑背景线程（录音/转写/热键）。
pub fn start_foreground(model: Option<PathBuf>) -> anyhow::Result<()> {
    let ui = Config::load()
        .map(|config| UiLanguage::from_config(&config))
        .unwrap_or_default();
    // ── 检查是否已在运行 ─────────────────────────────────────────────────
    if let Some(pid) = read_pid() {
        if is_running(pid) {
            println!(
                "{}",
                ui.pick(
                    format!("ℹ️  Open Flow 已在运行 (PID: {})", pid),
                    format!("ℹ️  Open Flow is already running (PID: {})", pid),
                )
            );
            println!(
                "   {}",
                ui.pick("停止: open-flow stop", "Stop: open-flow stop")
            );
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
        .block_on(model_store::ensure_model_ready(model))
        .map_err(|e| {
            eprintln!("❌ 模型准备失败: {}", e);
            e
        })?;

    // ── 持久化模型路径到配置文件（方便 status 命令展示）─────────────
    if let Ok(mut config) = Config::load() {
        config.model_path = Some(model_path.clone());
        let _ = config.save();
    }

    // ── 写 PID 文件（直接调用 --foreground 时写；后台启动时父进程已写）────
    let my_pid = std::process::id();
    let _ = fs::write(pid_path()?, my_pid.to_string());

    // ── 注册 Ctrl+C / 信号处理 → 设置 flag，由主循环正常退出 ─────────────────
    #[cfg(not(windows))]
    {
        // Unix: SIGINT/SIGTERM
        let _ = ctrlc::set_handler(move || {
            SIGNAL_SHUTDOWN.store(true, Ordering::SeqCst);
        });
    }
    #[cfg(windows)]
    {
        // Windows: SetConsoleCtrlHandler 必须返回 TRUE(1) 表示已处理，否则进程会被系统终止
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
        unsafe {
            let handler = Some(win32_ctrl_handler as _);
            SetConsoleCtrlHandler(handler, 1i32); // 1 = TRUE = add handler
        }
    }

    // ── 先初始化 AppKit / NSApplication，再创建托盘 ────────────────────
    // tray-icon 在 macOS 上要求主线程事件循环已开始处理事件后再创建 TrayIcon，
    // 否则状态图标可能根本不显示。
    #[cfg(target_os = "macos")]
    {
        prepare_appkit();
        pump_run_loop_100ms();
    }

    // ── 在主线程创建托盘（macOS 菜单栏 / Windows·Linux 系统托盘）────────────────────
    let (mut tray, mut tray_handle_raw) = match TrayState::new() {
        Ok((t, h)) => {
            t.set_state(TrayIconState::Idle);
            tracing::info!("✅ 托盘图标已创建");
            (Some(t), Some(h))
        }
        Err(e) => {
            tracing::warn!("托盘图标创建失败: {}，继续运行（无托盘）", e);
            (None, None)
        }
    };

    // ── 创建浮动指示器（overlay）──────────────────────────────────────
    use crate::overlay::OverlayWindow;
    let overlay = OverlayWindow::new();
    let (overlay_tx, overlay_rx) = std::sync::mpsc::sync_channel::<TrayIconState>(16);
    if let Some(ref mut handle) = tray_handle_raw {
        handle.set_overlay_sender(overlay_tx);
    }
    let tray_handle = tray_handle_raw.map(Arc::new);
    if overlay.is_some() {
        tracing::info!("✅ 浮动指示器已创建");
    }

    use crate::draft_panel::{DraftPanel, DraftPanelEvent};
    let draft_mode_active = Arc::new(AtomicBool::new(false));
    let draft_panel = DraftPanel::new(draft_mode_active.clone()).map(Arc::new);
    let (draft_tx, draft_rx) = std::sync::mpsc::sync_channel::<DraftPanelEvent>(64);
    if draft_panel.is_some() {
        tracing::info!("✅ 草稿面板已创建");
    }

    // ── Settings app 路径（与主二进制同目录）──────────────────────
    let settings_app_path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("OpenFlowSettings")))
        .filter(|p| p.exists());

    // ── 构建 ASR provider ──────────────────────────────────────────
    let config = Config::load().unwrap_or_default();
    let ui = UiLanguage::from_config(&config);
    let provider: Arc<dyn AsrProvider> = match config.provider.as_str() {
        "groq" => {
            let api_key = config.resolved_groq_api_key();
            match GroqAsrProvider::new(
                api_key,
                config.groq_model.clone(),
                config.groq_language.clone(),
            ) {
                Ok(p) => {
                    println!(
                        "   {}",
                        ui.pick(
                            format!("Provider: Groq ({})", config.groq_model),
                            format!("Provider: Groq ({})", config.groq_model),
                        )
                    );
                    Arc::new(p)
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        ui.pick(
                            format!("⚠️  Groq provider 初始化失败: {}。回退到本地模式。", e),
                            format!("⚠️  Failed to initialize Groq provider: {}. Falling back to local mode.", e),
                        )
                    );
                    Arc::new(LocalAsrProvider::new(model_path.clone()))
                }
            }
        }
        _ => {
            println!(
                "   {}",
                ui.pick(
                    "Provider: Local (SenseVoice)",
                    "Provider: Local (SenseVoice)"
                )
            );
            Arc::new(LocalAsrProvider::new(model_path.clone()))
        }
    };

    // ── 在专用线程运行 daemon（current_thread 运行时，Daemon 含 cpal::Stream 非 Send）
    let log = log_path()?;
    println!(
        "{}",
        ui.pick(
            format!("✅ Open Flow 已启动 (PID: {})", my_pid),
            format!("✅ Open Flow started (PID: {})", my_pid),
        )
    );
    println!("   {} {}", ui.pick("热键:", "Hotkey:"), config.hotkey);
    println!(
        "   {} {}",
        ui.pick("触发模式:", "Trigger Mode:"),
        config.trigger_mode
    );
    println!("   {} {}", ui.pick("日志:", "Log:"), log.display());
    println!();
    println!(
        "{}",
        ui.pick(
            "   按 Ctrl+C 或托盘菜单「退出」可停止",
            "   Press Ctrl+C or use the tray menu \"Quit\" to stop",
        )
    );
    println!(
        "{}",
        ui.pick(
            "   ⏳ 模型加载与预热约需 3-5 秒，完成后热键即可使用",
            "   ⏳ Model loading and warmup take about 3-5 seconds. The hotkey will work once ready.",
        )
    );

    let daemon_alive = Arc::new(AtomicBool::new(true));
    let daemon_alive_clone = daemon_alive.clone();
    let draft_mode_active_for_daemon = draft_mode_active.clone();

    let daemon_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("daemon tokio runtime");
        rt.block_on(async {
            if let Err(e) = run_daemon(
                provider,
                tray_handle,
                draft_mode_active_for_daemon,
                Some(draft_tx),
            )
            .await
            {
                eprintln!("Daemon 错误: {}", e);
            }
        });
        daemon_alive_clone.store(false, Ordering::SeqCst);
    });

    // ── 主线程：驱动 macOS NSRunLoop，让托盘 / 菜单事件得以分发 ──────
    run_main_loop(
        tray.as_ref(),
        overlay.as_ref(),
        &overlay_rx,
        draft_panel.as_deref(),
        &draft_rx,
        draft_mode_active.as_ref(),
        settings_app_path.as_deref(),
        &daemon_alive,
        my_pid,
    );

    // ── 退出前显式隐藏菜单栏图标并 pump run loop，避免图标残留
    if let Some(ref t) = tray {
        t.hide_from_menu_bar();
    }
    drop(tray.take());
    #[cfg(target_os = "macos")]
    for _ in 0..10 {
        pump_run_loop_100ms();
    }

    // ── 退出清理 ──────────────────────────────────────────────────────
    remove_pid_file();

    // 关闭设置应用（它在 .app bundle 内，不关闭会导致 bundle 被占用无法替换）
    let _ = std::process::Command::new("pkill")
        .args(["-x", "OpenFlowSettings"])
        .output();

    // 给 daemon 线程短时间优雅退出
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if daemon_handle.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("\n👋 Open Flow 已停止");

    // 强制退出——daemon 线程可能阻塞在 tokio recv() 或 CFRunLoop 上
    std::process::exit(0);
}

/// macOS 主循环：每 100ms 执行一次 NSRunLoop，检查是否需要退出。
/// 这是让 tray-icon 在 macOS 上正常渲染和响应菜单的关键。
fn run_main_loop(
    tray: Option<&TrayState>,
    overlay: Option<&crate::overlay::OverlayWindow>,
    overlay_rx: &std::sync::mpsc::Receiver<TrayIconState>,
    draft_panel: Option<&crate::draft_panel::DraftPanel>,
    draft_rx: &std::sync::mpsc::Receiver<crate::draft_panel::DraftPanelEvent>,
    draft_mode_active: &AtomicBool,
    settings_app_path: Option<&std::path::Path>,
    daemon_alive: &AtomicBool,
    current_pid: u32,
) {
    let ui_language = crate::common::config::Config::load()
        .map(|config| crate::common::ui::UiLanguage::from_config(&config))
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    let mut downloaded_update_zip: Option<PathBuf> = None;
    #[cfg(target_os = "macos")]
    let mut update_download_rx: Option<std::sync::mpsc::Receiver<UpdateDownloadEvent>> = None;

    if let Some(t) = tray {
        t.set_update_menu_text(ui_language.tray_update());
        t.set_update_menu_enabled(true);
        t.set_draft_menu_text(if draft_mode_active.load(Ordering::SeqCst) {
            ui_language.tray_draft_checked()
        } else {
            ui_language.tray_draft()
        });
    }

    loop {
        // 应用 daemon 发来的托盘状态更新（灰/红/黄）
        if let Some(t) = tray {
            t.flush_state_updates();
            t.flush_menu_events();
        }

        #[cfg(target_os = "macos")]
        if let Some(rx) = update_download_rx.take() {
            let mut keep_rx = true;

            loop {
                match rx.try_recv() {
                    Ok(UpdateDownloadEvent::Progress { percent }) => {
                        if let Some(t) = tray {
                            t.set_update_menu_text(&ui_language.tray_update_progress(percent));
                            t.set_update_menu_enabled(false);
                        }
                    }
                    Ok(UpdateDownloadEvent::Completed(Ok(UpdateDownloadResult::UpToDate {
                        latest_tag,
                    }))) => {
                        keep_rx = false;
                        if let Some(t) = tray {
                            t.set_update_menu_text(ui_language.tray_update());
                            t.set_update_menu_enabled(true);
                        }
                        show_update_popup(
                            ui_language,
                            &ui_language.pick(
                                format!("已是最新版本（{}）", latest_tag),
                                format!("You're already on the latest version ({})", latest_tag),
                            ),
                        );
                        break;
                    }
                    Ok(UpdateDownloadEvent::Completed(Ok(
                        UpdateDownloadResult::ReadyToInstall {
                            zip_path,
                            latest_tag,
                        },
                    ))) => {
                        keep_rx = false;
                        downloaded_update_zip = Some(zip_path);
                        if let Some(t) = tray {
                            t.set_update_menu_text(ui_language.tray_restart_to_apply_update());
                            t.set_update_menu_enabled(true);
                        }
                        show_update_popup(
                            ui_language,
                            &ui_language.pick(
                                format!("新版本 {} 安装包已下载，点击“重启以应用更新”开始安装。", latest_tag),
                                format!("The installer for {} is ready. Click \"Restart to Apply Update\" to begin installation.", latest_tag),
                            ),
                        );
                        break;
                    }
                    Ok(UpdateDownloadEvent::Completed(Err(err_msg))) => {
                        keep_rx = false;
                        eprintln!("[Updater] 更新任务失败：{}", err_msg);
                        if let Some(t) = tray {
                            t.set_update_menu_text(ui_language.tray_update());
                            t.set_update_menu_enabled(true);
                        }
                        show_update_popup(
                            ui_language,
                            &ui_language.pick(
                                format!("更新失败：{}", err_msg),
                                format!("Update failed: {}", err_msg),
                            ),
                        );
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        keep_rx = false;
                        if let Some(t) = tray {
                            t.set_update_menu_text(ui_language.tray_update());
                            t.set_update_menu_enabled(true);
                        }
                        show_update_popup(
                            ui_language,
                            ui_language.pick(
                                "更新任务中断，请重试。",
                                "The update task was interrupted. Please try again.",
                            ),
                        );
                        break;
                    }
                }
            }

            if keep_rx {
                update_download_rx = Some(rx);
            }
        }

        // 应用 overlay 状态更新
        while let Ok(state) = overlay_rx.try_recv() {
            if let Some(o) = overlay {
                o.update_state(state);
            }
        }

        while let Ok(event) = draft_rx.try_recv() {
            if let Some(panel) = draft_panel {
                match event {
                    crate::draft_panel::DraftPanelEvent::Show => panel.show(),
                    crate::draft_panel::DraftPanelEvent::Hide => panel.hide(),
                    crate::draft_panel::DraftPanelEvent::Clear => panel.clear(),
                    crate::draft_panel::DraftPanelEvent::SetText(text) => panel.set_text(&text),
                    crate::draft_panel::DraftPanelEvent::AppendText(text) => {
                        panel.append_text(&text)
                    }
                }
            }
        }

        if let Some(panel) = draft_panel {
            panel.poll_aux_windows();
        }

        if draft_mode_active.load(Ordering::SeqCst)
            && draft_panel
                .map(|panel| panel.consume_close_requested())
                .unwrap_or(false)
        {
            draft_mode_active.store(false, Ordering::SeqCst);
            if let Some(panel) = draft_panel {
                panel.set_draft_mode_enabled(false);
                panel.hide();
            }
        }

        if let Some(t) = tray {
            t.set_draft_menu_text(if draft_mode_active.load(Ordering::SeqCst) {
                ui_language.tray_draft_checked()
            } else {
                ui_language.tray_draft()
            });
        }

        // 驱动平台事件循环，确保托盘和菜单点击能被正确分发。
        #[cfg(target_os = "macos")]
        pump_run_loop_100ms();

        #[cfg(target_os = "windows")]
        pump_win32_messages();

        #[cfg(target_os = "linux")]
        pump_glib_linux();

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 托盘菜单「偏好设置...」-> 启动 SwiftUI 设置应用
        if tray.map_or(false, |t| t.prefs_requested()) {
            if let Some(path) = settings_app_path {
                let _ = std::process::Command::new(path).spawn();
            } else {
                tracing::warn!("设置应用未找到");
            }
        }

        if tray.map_or(false, |t| t.draft_requested()) {
            let next = !draft_mode_active.load(Ordering::SeqCst);
            draft_mode_active.store(next, Ordering::SeqCst);

            if let Some(panel) = draft_panel {
                panel.set_draft_mode_enabled(next);
                if next {
                    panel.show();
                } else {
                    panel.hide();
                }
            }
        }

        // 托盘菜单「检查更新」（仅 macOS .app）
        if tray.map_or(false, |t| t.update_requested()) {
            #[cfg(target_os = "macos")]
            if let Some(zip_path) = downloaded_update_zip.as_ref() {
                match start_install_downloaded_app_update(zip_path, current_pid) {
                    Ok(_) => break,
                    Err(e) => {
                        eprintln!("⚠️  自动升级失败: {}", e);
                        show_update_popup(
                            ui_language,
                            &ui_language.pick(
                                format!("自动升级失败：{}", e),
                                format!("Automatic update failed: {}", e),
                            ),
                        );
                    }
                }
            } else if update_download_rx.is_some() {
                show_update_popup(
                    ui_language,
                    ui_language.pick(
                        "正在后台下载更新包，请稍候。",
                        "The update package is downloading in the background. Please wait.",
                    ),
                );
            } else {
                let (tx, rx) = std::sync::mpsc::channel::<UpdateDownloadEvent>();
                update_download_rx = Some(rx);

                if let Some(t) = tray {
                    t.set_update_menu_text(ui_language.tray_update_downloading());
                    t.set_update_menu_enabled(false);
                }

                std::thread::spawn(move || {
                    let mut last_percent: u8 = 0;
                    let result = match check_and_download_app_update(|downloaded, total| {
                        if let Some(total_bytes) = total {
                            if total_bytes > 0 {
                                let percent =
                                    ((downloaded.saturating_mul(100)) / total_bytes).min(100) as u8;
                                if percent != last_percent {
                                    last_percent = percent;
                                    let _ = tx.send(UpdateDownloadEvent::Progress { percent });
                                }
                            }
                        }
                    }) {
                        Ok(result) => Ok(result),
                        Err(err) => {
                            log_update_error("更新检查或下载失败", &err);
                            Err(err.to_string())
                        }
                    };
                    let _ = tx.send(UpdateDownloadEvent::Completed(result));
                });
            }
        }

        // 托盘菜单「退出」
        if tray.map_or(false, |t| t.exit_requested()) {
            tracing::info!("用户点击托盘退出");
            break;
        }
        // SIGTERM / SIGINT（open-flow stop 或 Ctrl+C）
        if SIGNAL_SHUTDOWN.load(Ordering::SeqCst) {
            tracing::info!("收到信号，正常退出");
            break;
        }
        // daemon 线程意外退出
        if !daemon_alive.load(Ordering::SeqCst) {
            tracing::error!("Daemon 线程已意外退出");
            break;
        }
    }
}

/// Linux：处理 glib 主上下文，使托盘图标和菜单能响应点击。
#[cfg(target_os = "linux")]
fn pump_glib_linux() {
    let ctx = glib::MainContext::default();
    while ctx.iteration(false) {}
}

/// Windows：Ctrl+C/Ctrl+Break 控制台处理例程；返回 1(TRUE) 表示已处理，阻止系统默认终止进程。
#[cfg(windows)]
unsafe extern "system" fn win32_ctrl_handler(dw_ctrl_type: u32) -> i32 {
    if dw_ctrl_type == 0 /* CTRL_C_EVENT */ || dw_ctrl_type == 1
    /* CTRL_BREAK_EVENT */
    {
        SIGNAL_SHUTDOWN.store(true, Ordering::SeqCst);
        1i32 // TRUE：已处理，不要调用下一个 handler 或 ExitProcess
    } else {
        0i32 // FALSE：未处理，交给其他 handler
    }
}

/// Windows：处理当前线程消息队列，使托盘图标和菜单能响应点击。
#[cfg(target_os = "windows")]
fn pump_win32_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
    };

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // HWND/wMsgFilterMin/wMsgFilterMax 在 windows-sys 中为 isize/u32/u32，传 0 表示不过滤
    while unsafe { PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) } != 0 {
        if msg.message == WM_QUIT {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
/// 通过 `[NSApp nextEventMatchingMask:...]` 驱动 AppKit 事件队列。
/// NSRunLoop::runUntilDate 只处理 run loop sources，无法分发托盘点击事件；
/// 必须走 NSApplication 的事件队列才能响应 NSStatusItem 点击和菜单。
#[cfg(target_os = "macos")]
fn prepare_appkit() {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];

        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            // NSApplicationActivationPolicyAccessory = 1（无 Dock 图标，但可以有窗口）
            let _: () = msg_send![app, setActivationPolicy: 1i64];
            let _: () = msg_send![app, finishLaunching];
        });
    }
}

#[cfg(target_os = "macos")]
fn pump_run_loop_100ms() {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];

        prepare_appkit();

        let date_cls = class!(NSDate);
        // kCFRunLoopDefaultMode
        let mode: *mut Object = msg_send![
            class!(NSString),
            stringWithUTF8String: b"kCFRunLoopDefaultMode\0".as_ptr()
                as *const std::os::raw::c_char
        ];

        // 第一次调用最多等 100ms；之后排空剩余事件（distantPast = 不阻塞）
        let deadline: *mut Object = msg_send![date_cls, dateWithTimeIntervalSinceNow: 0.1f64];
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
    let ui = Config::load()
        .map(|config| UiLanguage::from_config(&config))
        .unwrap_or_default();
    let pid = match read_pid() {
        Some(p) => p,
        None => {
            println!(
                "{}",
                ui.pick(
                    "ℹ️  daemon 未运行（找不到 PID 文件）",
                    "ℹ️  daemon is not running (PID file not found)",
                )
            );
            return Ok(());
        }
    };

    if !is_running(pid) {
        println!(
            "{}",
            ui.pick(
                format!("ℹ️  daemon 未运行（PID {} 不存在）", pid),
                format!("ℹ️  daemon is not running (PID {} does not exist)", pid),
            )
        );
        remove_pid_file();
        return Ok(());
    }

    println!(
        "{}",
        ui.pick(
            format!("⏹️  正在停止 daemon (PID: {})...", pid),
            format!("⏹️  Stopping daemon (PID: {})...", pid),
        )
    );

    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !is_running(pid) {
                remove_pid_file();
                println!("{}", ui.pick("✅ daemon 已停止", "✅ daemon stopped"));
                return Ok(());
            }
        }
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        std::thread::sleep(std::time::Duration::from_millis(500));
        remove_pid_file();
        println!(
            "{}",
            ui.pick(
                "✅ daemon 已停止（强制终止）",
                "✅ daemon stopped (force killed)"
            )
        );
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, TerminateProcess, PROCESS_TERMINATE,
        };
        let h = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
        if h != 0 && h != -1_i32 as isize {
            unsafe {
                let _ = TerminateProcess(h as HANDLE, 0);
                CloseHandle(h as HANDLE);
            }
        }
        remove_pid_file();
        println!("{}", ui.pick("✅ daemon 已停止", "✅ daemon stopped"));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// status
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct PermissionStateSnapshot {
    status: String,
    granted: bool,
    can_prompt: bool,
    source: String,
}

#[derive(Serialize)]
struct PermissionSnapshot {
    accessibility: PermissionStateSnapshot,
    input_monitoring: PermissionStateSnapshot,
    microphone: PermissionStateSnapshot,
    current_exe: String,
}

pub async fn permissions(json: bool) -> Result<()> {
    let ui = Config::load()
        .map(|config| UiLanguage::from_config(&config))
        .unwrap_or_default();
    let current_exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<unavailable: {e}>"));
    let accessibility_ok = crate::hotkey::check_accessibility_permission();
    let input_monitoring_ok = crate::hotkey::check_input_monitoring_permission();
    let microphone_status = crate::hotkey::microphone_permission_status();
    let snapshot = PermissionSnapshot {
        accessibility: PermissionStateSnapshot {
            status: if accessibility_ok {
                "authorized".to_string()
            } else {
                "needs_manual_grant".to_string()
            },
            granted: accessibility_ok,
            can_prompt: !accessibility_ok,
            source: "ax_is_process_trusted".to_string(),
        },
        input_monitoring: PermissionStateSnapshot {
            status: if input_monitoring_ok {
                "authorized".to_string()
            } else {
                "needs_manual_grant".to_string()
            },
            granted: input_monitoring_ok,
            can_prompt: !input_monitoring_ok,
            source: "cg_preflight_listen_event_access".to_string(),
        },
        microphone: PermissionStateSnapshot {
            status: microphone_status.as_str().to_string(),
            granted: microphone_status.is_authorized(),
            can_prompt: microphone_status.can_prompt(),
            source: "avcapturedevice.authorization_status".to_string(),
        },
        current_exe,
    };

    if json {
        println!("{}", serde_json::to_string(&snapshot)?);
        return Ok(());
    }

    println!(
        "{}",
        ui.pick("Open Flow 权限状态", "Open Flow Permission Status")
    );
    println!(
        "  {} {}",
        ui.pick("可执行文件:", "Executable:"),
        snapshot.current_exe
    );
    println!(
        "  Accessibility: {} ({})",
        snapshot.accessibility.granted, snapshot.accessibility.status
    );
    println!(
        "  Input Monitoring: {} ({})",
        snapshot.input_monitoring.granted, snapshot.input_monitoring.status
    );
    println!(
        "  Microphone: {} ({})",
        snapshot.microphone.granted, snapshot.microphone.status
    );
    Ok(())
}

pub async fn status() -> Result<()> {
    let config = Config::load()?;
    let ui = UiLanguage::from_config(&config);

    match read_pid() {
        Some(pid) if is_running(pid) => {
            let uptime = get_uptime_str(pid);
            println!(
                "{}",
                ui.pick("Open Flow daemon 状态", "Open Flow daemon status")
            );
            println!(
                "  {}     {}",
                ui.pick("状态:", "Status:"),
                ui.pick("✅ 运行中", "✅ Running")
            );
            println!("  PID:      {}", pid);
            println!("  {}     {}", ui.pick("运行:", "Uptime:"), uptime);
            println!(
                "  {}     {:?}",
                ui.pick("模型:", "Model:"),
                config.model_path.unwrap_or_default()
            );
            println!("  Provider: {}", config.provider);
            println!("  {}     {}", ui.pick("热键:", "Hotkey:"), config.hotkey);
            println!(
                "  {} {}",
                ui.pick("触发模式:", "Trigger Mode:"),
                config.trigger_mode
            );
            println!(
                "  {}     {}",
                ui.pick("日志:", "Log:"),
                log_path()?.display()
            );
        }
        Some(pid) => {
            println!(
                "{}",
                ui.pick("Open Flow daemon 状态", "Open Flow daemon status")
            );
            println!(
                "  {}   {}",
                ui.pick("状态:", "Status:"),
                ui.pick(
                    format!("❌ 未运行（PID {} 已失效）", pid),
                    format!("❌ Not running (PID {} is stale)", pid),
                )
            );
            remove_pid_file();
        }
        None => {
            println!(
                "{}",
                ui.pick("Open Flow daemon 状态", "Open Flow daemon status")
            );
            println!(
                "  {}   {}",
                ui.pick("状态:", "Status:"),
                ui.pick("❌ 未运行", "❌ Not running")
            );
            println!("  {}   open-flow start", ui.pick("启动:", "Start:"));
        }
    }
    Ok(())
}

/// 用 ps 获取进程启动时间（仅用于展示；Windows 返回 N/A）
fn get_uptime_str(pid: u32) -> String {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "etime="])
            .output();
        match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => "未知".to_string(),
        }
    }
    #[cfg(windows)]
    {
        let _ = pid;
        "N/A".to_string()
    }
}
