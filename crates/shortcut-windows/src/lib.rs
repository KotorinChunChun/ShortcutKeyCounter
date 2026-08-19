//! Windows固有の入力取得・ライフサイクル連携を提供するクレート。

pub mod autostart;
pub mod elevation;
pub mod foreground;
pub mod ipc;
pub mod keyboard_hook;
pub mod known_folders;
pub mod modifier_state;
pub mod mouse_hook;
pub mod power_monitor;
pub mod pressed_keys;
pub mod user_activity;

pub mod shell_open;

pub mod single_instance;

pub mod overlay_window;
