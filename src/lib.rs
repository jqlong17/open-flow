#![allow(unexpected_cfgs)]

pub const IS_MAS_BUILD: bool = cfg!(feature = "mas");

pub mod asr;
pub mod audio;
pub mod common;
pub mod correction;
pub mod draft_panel;
pub mod draft_transform;
pub mod hotkey;
pub mod llm;
pub mod meeting_notes;
pub mod model_store;
pub mod overlay;
pub mod settings_window;
pub mod system_audio;
pub mod text_injection;
pub mod tray;
