//! 托盘图标：macOS 菜单栏、Windows·Linux 系统托盘（三态 + 菜单）；其他平台为 stub。

/// 托盘状态：待机 / 录音中 / 转写中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconState {
    Idle,
    Recording,
    Transcribing,
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS 完整实现
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::TrayIconState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder};
    use tracing::info;

    const ICON_SIZE: u32 = 22;
    const DOT_RADIUS: f32 = 4.0;

    /// Send+Sync 句柄，供 daemon 背景线程使用
    pub struct TrayHandle {
        pub(super) state_tx: std::sync::mpsc::SyncSender<TrayIconState>,
        pub(super) exit_requested: Arc<AtomicBool>,
    }

    impl TrayHandle {
        pub fn set_state(&self, state: TrayIconState) {
            let _ = self.state_tx.try_send(state);
        }
        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }
    }

    /// 托盘图标与菜单（只能在主线程使用）
    pub struct TrayState {
        tray_icon: tray_icon::TrayIcon,
        icon_idle: Icon,
        icon_recording: Icon,
        icon_transcribing: Icon,
        status_item: MenuItem,
        pub exit_requested: Arc<AtomicBool>,
        state_rx: std::sync::mpsc::Receiver<TrayIconState>,
    }

    impl TrayState {
        pub fn new() -> Result<(Self, TrayHandle), tray_icon::Error> {
            let icon_idle = create_circle_icon(128, 128, 128);
            let icon_recording = create_circle_icon(255, 80, 80);
            let icon_transcribing = create_circle_icon(255, 200, 0);

            let exit_requested = Arc::new(AtomicBool::new(false));
            let (state_tx, state_rx) = std::sync::mpsc::sync_channel::<TrayIconState>(16);

            let title = MenuItem::with_id("title", format!("Open Flow  v{}", env!("CARGO_PKG_VERSION")), true, None);
            let status_item = MenuItem::with_id("status", "状态：待机", false, None);
            let exit = MenuItem::with_id("exit", "退出", true, None);

            let menu = Menu::with_items(&[&title, &status_item, &exit])
                .map_err(|e| tray_icon::Error::OsError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

            let tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Open Flow - 语音输入")
                .with_icon(icon_idle.clone())
                .build()?;

            let handle = TrayHandle { state_tx, exit_requested: exit_requested.clone() };
            let tray_state = Self {
                tray_icon, icon_idle, icon_recording, icon_transcribing,
                status_item, exit_requested, state_rx,
            };
            Ok((tray_state, handle))
        }

        pub fn set_state(&self, state: TrayIconState) {
            self.apply_state(state);
        }

        pub fn flush_state_updates(&self) {
            while let Ok(state) = self.state_rx.try_recv() {
                self.apply_state(state);
            }
        }

        pub fn flush_menu_events(&self) {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id.as_ref() == "exit" {
                    info!("用户点击退出菜单");
                    self.exit_requested.store(true, Ordering::SeqCst);
                } else if event.id.as_ref() == "title" {
                    let _ = std::process::Command::new("open")
                        .arg("https://github.com/jqlong17/open-flow")
                        .spawn();
                }
            }
        }

        fn apply_state(&self, state: TrayIconState) {
            let (icon, text) = match state {
                TrayIconState::Idle => (&self.icon_idle, "状态：待机"),
                TrayIconState::Recording => (&self.icon_recording, "状态：录音中"),
                TrayIconState::Transcribing => (&self.icon_transcribing, "状态：转写中"),
            };
            if let Err(e) = self.tray_icon.set_icon(Some(icon.clone())) {
                tracing::warn!("更新托盘图标失败: {:?}", e);
            }
            self.status_item.set_text(text);
        }

        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }

        pub fn hide_from_menu_bar(&self) {
            if let Err(e) = self.tray_icon.set_visible(false) {
                tracing::warn!("隐藏菜单栏图标失败: {:?}", e);
            }
        }
    }

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Windows：系统托盘（任务栏右侧），三态图标 + 菜单，需主线程跑 Win32 消息循环
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::TrayIconState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder};
    use tracing::info;

    const ICON_SIZE: u32 = 22;
    const DOT_RADIUS: f32 = 4.0;

    pub struct TrayHandle {
        pub(super) state_tx: std::sync::mpsc::SyncSender<TrayIconState>,
        pub(super) exit_requested: Arc<AtomicBool>,
    }

    impl TrayHandle {
        pub fn set_state(&self, state: TrayIconState) {
            let _ = self.state_tx.try_send(state);
        }
        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }
    }

    pub struct TrayState {
        tray_icon: tray_icon::TrayIcon,
        icon_idle: Icon,
        icon_recording: Icon,
        icon_transcribing: Icon,
        status_item: MenuItem,
        pub exit_requested: Arc<AtomicBool>,
        state_rx: std::sync::mpsc::Receiver<TrayIconState>,
    }

    impl TrayState {
        pub fn new() -> Result<(Self, TrayHandle), tray_icon::Error> {
            let icon_idle = create_circle_icon(128, 128, 128);
            let icon_recording = create_circle_icon(255, 80, 80);
            let icon_transcribing = create_circle_icon(255, 200, 0);

            let exit_requested = Arc::new(AtomicBool::new(false));
            let (state_tx, state_rx) = std::sync::mpsc::sync_channel::<TrayIconState>(16);

            let title = MenuItem::with_id("title", format!("Open Flow  v{}", env!("CARGO_PKG_VERSION")), true, None);
            let status_item = MenuItem::with_id("status", "状态：待机", false, None);
            let exit = MenuItem::with_id("exit", "退出", true, None);

            let menu = Menu::with_items(&[&title, &status_item, &exit])
                .map_err(|e| tray_icon::Error::OsError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

            let tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Open Flow - 语音输入")
                .with_icon(icon_idle.clone())
                .build()?;

            let handle = TrayHandle { state_tx, exit_requested: exit_requested.clone() };
            let tray_state = Self {
                tray_icon, icon_idle, icon_recording, icon_transcribing,
                status_item, exit_requested, state_rx,
            };
            Ok((tray_state, handle))
        }

        pub fn set_state(&self, state: TrayIconState) {
            self.apply_state(state);
        }

        pub fn flush_state_updates(&self) {
            while let Ok(state) = self.state_rx.try_recv() {
                self.apply_state(state);
            }
        }

        pub fn flush_menu_events(&self) {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id.as_ref() == "exit" {
                    info!("用户点击退出菜单");
                    self.exit_requested.store(true, Ordering::SeqCst);
                } else if event.id.as_ref() == "title" {
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", "", "https://github.com/jqlong17/open-flow"])
                        .spawn();
                }
            }
        }

        fn apply_state(&self, state: TrayIconState) {
            let (icon, text) = match state {
                TrayIconState::Idle => (&self.icon_idle, "状态：待机"),
                TrayIconState::Recording => (&self.icon_recording, "状态：录音中"),
                TrayIconState::Transcribing => (&self.icon_transcribing, "状态：转写中"),
            };
            if let Err(e) = self.tray_icon.set_icon(Some(icon.clone())) {
                tracing::warn!("更新托盘图标失败: {:?}", e);
            }
            self.status_item.set_text(text);
        }

        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }

        pub fn hide_from_menu_bar(&self) {
            if let Err(e) = self.tray_icon.set_visible(false) {
                tracing::warn!("隐藏托盘图标失败: {:?}", e);
            }
        }
    }

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Linux：系统托盘（通知区域），三态图标 + 菜单，需主线程跑 glib 事件循环
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    use super::TrayIconState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder};
    use tracing::info;

    const ICON_SIZE: u32 = 22;
    const DOT_RADIUS: f32 = 4.0;

    pub struct TrayHandle {
        pub(super) state_tx: std::sync::mpsc::SyncSender<TrayIconState>,
        pub(super) exit_requested: Arc<AtomicBool>,
    }

    impl TrayHandle {
        pub fn set_state(&self, state: TrayIconState) {
            let _ = self.state_tx.try_send(state);
        }
        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }
    }

    pub struct TrayState {
        tray_icon: tray_icon::TrayIcon,
        icon_idle: Icon,
        icon_recording: Icon,
        icon_transcribing: Icon,
        status_item: MenuItem,
        pub exit_requested: Arc<AtomicBool>,
        state_rx: std::sync::mpsc::Receiver<TrayIconState>,
    }

    impl TrayState {
        pub fn new() -> Result<(Self, TrayHandle), tray_icon::Error> {
            let icon_idle = create_circle_icon(128, 128, 128);
            let icon_recording = create_circle_icon(255, 80, 80);
            let icon_transcribing = create_circle_icon(255, 200, 0);

            let exit_requested = Arc::new(AtomicBool::new(false));
            let (state_tx, state_rx) = std::sync::mpsc::sync_channel::<TrayIconState>(16);

            let title = MenuItem::with_id("title", format!("Open Flow  v{}", env!("CARGO_PKG_VERSION")), true, None);
            let status_item = MenuItem::with_id("status", "状态：待机", false, None);
            let exit = MenuItem::with_id("exit", "退出", true, None);

            let menu = Menu::with_items(&[&title, &status_item, &exit])
                .map_err(|e| tray_icon::Error::OsError(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

            let tray_icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Open Flow - 语音输入")
                .with_icon(icon_idle.clone())
                .build()?;

            let handle = TrayHandle { state_tx, exit_requested: exit_requested.clone() };
            let tray_state = Self {
                tray_icon, icon_idle, icon_recording, icon_transcribing,
                status_item, exit_requested, state_rx,
            };
            Ok((tray_state, handle))
        }

        pub fn set_state(&self, state: TrayIconState) {
            self.apply_state(state);
        }

        pub fn flush_state_updates(&self) {
            while let Ok(state) = self.state_rx.try_recv() {
                self.apply_state(state);
            }
        }

        pub fn flush_menu_events(&self) {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id.as_ref() == "exit" {
                    info!("用户点击退出菜单");
                    self.exit_requested.store(true, Ordering::SeqCst);
                } else if event.id.as_ref() == "title" {
                    let _ = std::process::Command::new("xdg-open")
                        .arg("https://github.com/jqlong17/open-flow")
                        .spawn();
                }
            }
        }

        fn apply_state(&self, state: TrayIconState) {
            let (icon, text) = match state {
                TrayIconState::Idle => (&self.icon_idle, "状态：待机"),
                TrayIconState::Recording => (&self.icon_recording, "状态：录音中"),
                TrayIconState::Transcribing => (&self.icon_transcribing, "状态：转写中"),
            };
            if let Err(e) = self.tray_icon.set_icon(Some(icon.clone())) {
                tracing::warn!("更新托盘图标失败: {:?}", e);
            }
            self.status_item.set_text(text);
        }

        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }

        pub fn hide_from_menu_bar(&self) {
            if let Err(e) = self.tray_icon.set_visible(false) {
                tracing::warn!("隐藏托盘图标失败: {:?}", e);
            }
        }
    }

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
}

// ─────────────────────────────────────────────────────────────────────────────
// 非 macOS / Windows / Linux：无操作 stub
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    use super::TrayIconState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    pub struct TrayHandle {
        pub(super) exit_requested: Arc<AtomicBool>,
    }

    impl TrayHandle {
        pub fn set_state(&self, _state: TrayIconState) {}
        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }
    }

    pub struct TrayState {
        pub exit_requested: Arc<AtomicBool>,
    }

    impl TrayState {
        pub fn new() -> Result<(Self, TrayHandle), tray_icon::Error> {
            let flag = Arc::new(AtomicBool::new(false));
            Ok((
                Self { exit_requested: flag.clone() },
                TrayHandle { exit_requested: flag },
            ))
        }
        pub fn set_state(&self, _state: TrayIconState) {}
        pub fn flush_state_updates(&self) {}
        pub fn flush_menu_events(&self) {}
        pub fn exit_requested(&self) -> bool {
            self.exit_requested.load(Ordering::SeqCst)
        }
        pub fn hide_from_menu_bar(&self) {}
    }
}

// 把平台实现统一 re-export，调用方无需关心平台
pub use platform::{TrayHandle, TrayState};
