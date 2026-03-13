//! macOS 菜单栏托盘图标：待机 / 录音中 / 转写中 三态 + 右键菜单

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use tracing::info;

// ─────────────────────────────────────────────────────────────────────────────
// TrayHandle：Send+Sync 的轻量句柄，供 daemon 背景线程使用
// ─────────────────────────────────────────────────────────────────────────────

/// 状态更新请求通道（daemon → 主线程）+ 退出标志
pub struct TrayHandle {
    state_tx: std::sync::mpsc::SyncSender<TrayIconState>,
    exit_requested: Arc<AtomicBool>,
}

impl TrayHandle {
    pub fn set_state(&self, state: TrayIconState) {
        let _ = self.state_tx.try_send(state);
    }
    pub fn exit_requested(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }
}

const ICON_SIZE: u32 = 22;
/// 圆点半径（像素），越小越低调；4 ≈ 小圆点，9 ≈ 大圆
const DOT_RADIUS: f32 = 4.0;

/// 托盘状态：待机 / 录音中 / 转写中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconState {
    Idle,
    Recording,
    Transcribing,
}

/// 托盘图标与菜单，支持三态切换（只能在主线程使用）
pub struct TrayState {
    tray_icon: tray_icon::TrayIcon,
    icon_idle: Icon,
    icon_recording: Icon,
    icon_transcribing: Icon,
    /// 收到 Exit 菜单点击时设为 true
    pub exit_requested: Arc<AtomicBool>,
    /// 接收来自 daemon 背景线程的状态更新
    state_rx: std::sync::mpsc::Receiver<TrayIconState>,
}

impl TrayState {
    /// 创建托盘并返回 (TrayState, TrayHandle)。
    /// TrayState 留在主线程；TrayHandle 可 Send 给 daemon 背景线程。
    pub fn new() -> Result<(Self, TrayHandle), tray_icon::Error> {
        let icon_idle = create_circle_icon(128, 128, 128); // 灰色
        let icon_recording = create_circle_icon(255, 80, 80); // 红色
        let icon_transcribing = create_circle_icon(255, 200, 0); // 黄色

        let exit_requested = Arc::new(AtomicBool::new(false));
        let exit_clone = exit_requested.clone();

        let (state_tx, state_rx) = std::sync::mpsc::sync_channel::<TrayIconState>(16);

        // 菜单项直接挂在根 Menu 上，点击托盘图标即可看到
        let menu = Menu::with_items(&[
            &MenuItem::with_id("title", format!("Open Flow  v{}", env!("CARGO_PKG_VERSION")), false, None),
            &MenuItem::with_id("status", "状态：待机", false, None),
            &MenuItem::with_id("exit", "退出", true, None),
        ]).map_err(|e| tray_icon::Error::OsError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Open Flow - 语音输入")
            .with_icon(icon_idle.clone())
            .build()?;

        // 监听菜单点击
        std::thread::spawn(move || {
            let receiver = MenuEvent::receiver();
            loop {
                match receiver.recv() {
                    Ok(event) => {
                        if event.id.as_ref() == "exit" {
                            info!("用户点击退出菜单");
                            exit_clone.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let handle = TrayHandle {
            state_tx,
            exit_requested: exit_requested.clone(),
        };

        let tray_state = Self {
            tray_icon,
            icon_idle,
            icon_recording,
            icon_transcribing,
            exit_requested,
            state_rx,
        };

        Ok((tray_state, handle))
    }

    /// 应用图标状态（直接调用）
    pub fn set_state(&self, state: TrayIconState) {
        self.apply_state(state);
    }

    /// 从 channel 拉取并应用 daemon 发来的状态更新
    pub fn flush_state_updates(&self) {
        while let Ok(state) = self.state_rx.try_recv() {
            self.apply_state(state);
        }
    }

    fn apply_state(&self, state: TrayIconState) {
        let icon = match state {
            TrayIconState::Idle => &self.icon_idle,
            TrayIconState::Recording => &self.icon_recording,
            TrayIconState::Transcribing => &self.icon_transcribing,
        };
        if let Err(e) = self.tray_icon.set_icon(Some(icon.clone())) {
            tracing::warn!("更新托盘图标失败: {:?}", e);
        }
    }

    pub fn exit_requested(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }
}

/// 创建 22x22 的小圆点 RGBA 图标（带抗锯齿边缘，DOT_RADIUS 控制大小）
fn create_circle_icon(r: u8, g: u8, b: u8) -> Icon {
    let size = ICON_SIZE as usize;
    let center = size as f32 / 2.0 - 0.5;

    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = (y * size + x) * 4;
            // 抗锯齿：在边缘 1px 范围内渐变 alpha
            let alpha = ((DOT_RADIUS + 0.5 - dist).clamp(0.0, 1.0) * 255.0) as u8;
            if alpha > 0 {
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = alpha;
            }
        }
    }
    Icon::from_rgba(rgba, ICON_SIZE, ICON_SIZE).expect("icon")
}
