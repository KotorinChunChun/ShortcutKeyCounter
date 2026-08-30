use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use shortcut_core::key_names::key_catalog;
use thiserror::Error;

const DEFAULT_KEY_SVG: &str = include_str!("../../resources/overlay/default/key.svg");
const DEFAULT_MODIFIER_SVG: &str = include_str!("../../resources/overlay/default/modifier.svg");
const DEFAULT_MOUSE_SVG: &str = include_str!("../../resources/overlay/default/mouse.svg");
const DEFAULT_BACKGROUND: &str = "#202020";

pub use shortcut_common::overlay::OverlayPosition;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayDisplayMode {
    #[default]
    Desktop,
    ActiveWindow,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayMonitorMode {
    #[default]
    ActiveWindow,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPositionMode {
    #[default]
    Fixed,
    Auto,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayInAnimation {
    None,
    Fade,
    #[default]
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Pop,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayOutAnimation {
    None,
    #[default]
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Shrink,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverlayBackgroundConfig {
    pub color: String,
    pub opacity: f32,
}

impl Default for OverlayBackgroundConfig {
    fn default() -> Self {
        Self {
            color: DEFAULT_BACKGROUND.to_owned(),
            opacity: 0.80,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverlayAnimationConfig {
    pub enter: OverlayInAnimation,
    pub exit: OverlayOutAnimation,
    pub enter_ms: u64,
    pub exit_ms: u64,
    pub slide_distance: f32,
    pub start_scale: f32,
    #[serde(default)]
    pub evict_ms: u64,
}

impl Default for OverlayAnimationConfig {
    fn default() -> Self {
        Self {
            enter: OverlayInAnimation::SlideUp,
            exit: OverlayOutAnimation::Fade,
            enter_ms: 160,
            exit_ms: 400,
            slide_distance: 18.0,
            start_scale: 0.96,
            evict_ms: 120,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KeyConfig {
    pub height: u32,
    pub min_width: u32,
    pub padding_x: u32,
    pub font_size: f32,
    pub text_color: String,
    pub separator_color: String,
    pub separator_gap: u32,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            height: 34,
            min_width: 42,
            padding_x: 12,
            font_size: 18.0,
            text_color: "#20252B".to_owned(),
            separator_color: "#F4F7FA".to_owned(),
            separator_gap: 9,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepeatBadgeShape {
    #[default]
    Pill,
    Rounded,
    Square,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct RepeatCounterTheme {
    pub background_color: String,
    pub text_color: String,
    pub accent_color: String,
    #[serde(default)]
    pub shape: RepeatBadgeShape,
    pub rotation_deg: f32,
    pub font_size: f32,
}

impl Default for RepeatCounterTheme {
    fn default() -> Self {
        Self {
            background_color: "#FF4F8B".to_owned(),
            text_color: "#FFFFFF".to_owned(),
            accent_color: "#B9275B".to_owned(),
            shape: RepeatBadgeShape::Pill,
            rotation_deg: -3.0,
            font_size: 14.0,
        }
    }
}

/// v0.1.2のOverlay設定。
///
/// `enabled`はv0.1.1 JSONとの読み込み互換のためだけに残している。v2 JSONへは
/// 書き出さず、実行時のON/OFFはSQLite `settings.overlay_enabled`を正本とする。
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OverlayConfig {
    pub version: u32,
    #[serde(default = "default_enabled", skip_serializing)]
    pub enabled: bool,
    #[serde(default = "default_design_directory")]
    pub design_directory: String,
    #[serde(default)]
    pub key: KeyConfig,
    #[serde(default = "default_display_duration_ms")]
    pub display_duration_ms: u64,
    #[serde(default)]
    pub display_mode: OverlayDisplayMode,
    #[serde(default)]
    pub monitor_mode: OverlayMonitorMode,
    #[serde(default)]
    pub fixed_monitor_id: String,
    #[serde(default)]
    pub position_mode: OverlayPositionMode,
    #[serde(default)]
    pub fixed_position: OverlayPosition,
    /// Active-window avoidance mode candidate positions. v0.1.2 auto-rotation state is obsolete.
    #[serde(default = "default_allowed_positions")]
    pub allowed_positions: Vec<OverlayPosition>,
    #[serde(default)]
    pub preferred_position: OverlayPosition,
    #[serde(default = "default_margin_px")]
    pub margin_px: i32,
    #[serde(default = "default_item_gap_px")]
    pub item_gap_px: i32,
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
    #[serde(default)]
    pub background: OverlayBackgroundConfig,
    #[serde(default)]
    pub animation: OverlayAnimationConfig,
    #[serde(default)]
    pub repeat_counter: RepeatCounterTheme,
    #[serde(default = "default_cache_items")]
    pub cache_items: usize,
    #[serde(default = "default_max_window_width")]
    pub max_window_width: u32,
}

/// 既存v0.1.1コードとの互換名。新しいコードではOverlayConfigを使う。
pub type OverlayThemeConfig = OverlayConfig;

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            version: 3,
            enabled: true,
            design_directory: default_design_directory(),
            key: KeyConfig::default(),
            display_duration_ms: default_display_duration_ms(),
            display_mode: OverlayDisplayMode::Desktop,
            monitor_mode: OverlayMonitorMode::ActiveWindow,
            fixed_monitor_id: String::new(),
            position_mode: OverlayPositionMode::Fixed,
            fixed_position: OverlayPosition::BottomCenter,
            allowed_positions: default_allowed_positions(),
            preferred_position: OverlayPosition::BottomCenter,
            margin_px: default_margin_px(),
            item_gap_px: default_item_gap_px(),
            max_rows: default_max_rows(),
            background: OverlayBackgroundConfig::default(),
            animation: OverlayAnimationConfig::default(),
            repeat_counter: RepeatCounterTheme::default(),
            cache_items: default_cache_items(),
            max_window_width: default_max_window_width(),
        }
    }
}

impl OverlayConfig {
    /// 現在の配置・表示先などを維持したまま、描画・外観関連だけを既定値へ戻す。
    pub fn reset_appearance_from_defaults(mut self) -> Self {
        let defaults = Self::default();
        self.design_directory = defaults.design_directory;
        self.key = defaults.key;
        self.display_duration_ms = defaults.display_duration_ms;
        self.margin_px = defaults.margin_px;
        self.item_gap_px = defaults.item_gap_px;
        self.max_rows = defaults.max_rows;
        self.background = defaults.background;
        self.animation = defaults.animation;
        self.repeat_counter = defaults.repeat_counter;
        self.cache_items = defaults.cache_items;
        self.max_window_width = defaults.max_window_width;
        self.sanitize()
    }

    pub fn total_lifetime_ms(&self) -> u64 {
        self.animation
            .enter_ms
            .saturating_add(self.display_duration_ms)
            .saturating_add(self.animation.exit_ms)
    }

    pub fn sanitize(mut self) -> Self {
        self.version = 3;
        self.display_duration_ms = self.display_duration_ms.clamp(100, 10_000);
        self.margin_px = self.margin_px.clamp(0, 500);
        self.item_gap_px = self.item_gap_px.clamp(0, 64);
        self.max_rows = self.max_rows.clamp(1, 10);
        self.background.opacity = self.background.opacity.clamp(0.0, 1.0);
        self.animation.enter_ms = self.animation.enter_ms.clamp(0, 2_000);
        self.animation.exit_ms = self.animation.exit_ms.clamp(0, 2_000);
        self.animation.slide_distance = self.animation.slide_distance.clamp(0.0, 200.0);
        self.animation.start_scale = self.animation.start_scale.clamp(0.5, 1.5);
        self.animation.evict_ms = self.animation.evict_ms.clamp(0, 2_000);
        self.repeat_counter.rotation_deg = self.repeat_counter.rotation_deg.clamp(-20.0, 20.0);
        self.repeat_counter.font_size = self.repeat_counter.font_size.clamp(8.0, 32.0);
        self.cache_items = self.cache_items.clamp(8, 512);
        // Kept only for backward-compatible JSON loading. Rendering now uses monitor work area.
        self.max_window_width = self.max_window_width.clamp(240, 16_000);
        let mut allowed_positions = Vec::with_capacity(self.allowed_positions.len());
        for position in self.allowed_positions.drain(..) {
            if !allowed_positions.contains(&position) {
                allowed_positions.push(position);
            }
        }
        self.allowed_positions = allowed_positions;
        if self.allowed_positions.is_empty() {
            self.allowed_positions = default_allowed_positions();
        }
        if self.fixed_monitor_id.len() > 128 {
            self.fixed_monitor_id = self.fixed_monitor_id.chars().take(128).collect();
        }

        self.key.height = self.key.height.clamp(28, 120);
        self.key.min_width = self.key.min_width.clamp(28, 220);
        self.key.padding_x = self.key.padding_x.clamp(4, 64);
        self.key.font_size = self.key.font_size.clamp(10.0, 56.0);
        self.key.separator_gap = self.key.separator_gap.clamp(2, 32);

        if !is_hex_color(&self.background.color) {
            self.background.color = DEFAULT_BACKGROUND.to_owned();
        }
        if !is_hex_color(&self.repeat_counter.background_color) {
            self.repeat_counter.background_color = RepeatCounterTheme::default().background_color;
        }
        if !is_hex_color(&self.repeat_counter.text_color) {
            self.repeat_counter.text_color = RepeatCounterTheme::default().text_color;
        }
        if !is_hex_color(&self.repeat_counter.accent_color) {
            self.repeat_counter.accent_color = RepeatCounterTheme::default().accent_color;
        }
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OverlayKeyLabels {
    pub version: u32,
    #[serde(default)]
    pub keyboard_layout: KeyboardLayout,
    pub keys: BTreeMap<String, String>,
}

/// オーバーレイ表示名の既定値に使用するキーボード配列。
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyboardLayout {
    #[default]
    Japanese109,
    Us104,
}

impl Default for OverlayKeyLabels {
    fn default() -> Self {
        Self::for_layout(KeyboardLayout::default())
    }
}

impl OverlayKeyLabels {
    pub fn for_layout(keyboard_layout: KeyboardLayout) -> Self {
        let mut keys = BTreeMap::new();
        for (_, name) in key_catalog() {
            keys.insert(name.clone(), name);
        }
        for name in [
            "MouseLeft",
            "MouseRight",
            "MouseMiddle",
            "MouseX1",
            "MouseX2",
            "WheelUp",
            "WheelDown",
            "WheelLeft",
            "WheelRight",
        ] {
            keys.insert(name.to_owned(), name.to_owned());
        }
        if keyboard_layout == KeyboardLayout::Japanese109 {
            apply_japanese109_defaults(&mut keys);
        }
        Self {
            version: 2,
            keyboard_layout,
            keys,
        }
    }

    pub fn merge_with_defaults(mut saved: Self) -> Self {
        let mut current = Self::for_layout(saved.keyboard_layout);
        current.keys.append(&mut saved.keys);
        current.version = 2;
        current
    }

    /// 配列変更時に、旧配列の既定値だけを新配列の既定値へ置換する。
    /// ユーザーが編集した表示名は保持する。
    pub fn with_layout(mut self, keyboard_layout: KeyboardLayout) -> Self {
        if self.keyboard_layout == keyboard_layout {
            return Self::merge_with_defaults(self);
        }
        let old_defaults = Self::for_layout(self.keyboard_layout).keys;
        let new_defaults = Self::for_layout(keyboard_layout).keys;
        for (key, old_label) in old_defaults {
            if self.keys.get(&key) == Some(&old_label) {
                if let Some(new_label) = new_defaults.get(&key) {
                    self.keys.insert(key, new_label.clone());
                }
            }
        }
        self.keyboard_layout = keyboard_layout;
        Self::merge_with_defaults(self)
    }

    pub fn display<'a>(&'a self, internal: &'a str) -> &'a str {
        self.keys
            .get(internal)
            .map(String::as_str)
            .unwrap_or(internal)
    }
}

fn apply_japanese109_defaults(keys: &mut BTreeMap<String, String>) {
    // Windows VK names remain stable internal identities; only the
    // user-facing overlay labels change for the Japanese 109-key layout.
    let overrides = [
        ("Cancel", "Break"),
        ("Kana", "かな"),
        ("Kanji", "Alt+半角／全角"),
        ("Convert", "変換"),
        ("NonConvert", "無変換"),
        ("Semicolon", "コロン"),
        ("Equal", "セミコロン"),
        ("Backquote", "＠"),
        ("LeftBracket", "［"),
        ("Backslash", "￥"),
        ("RightBracket", "］"),
        ("Quote", "＾"),
        ("Oem102", "＼"),
        ("OemAttn", "英数"),
        ("OemCopy", "カタカナ／ひらがな"),
        ("OemAuto", "半角／全角"),
        ("OemBackTab", "Alt+カタカナ／ひらがな"),
    ];
    for (key, label) in overrides {
        if keys.contains_key(key) {
            keys.insert(key.to_owned(), label.to_owned());
        }
    }
}

#[derive(Clone, Debug)]
pub struct OverlayPaths {
    pub theme_json: PathBuf,
    pub labels_json: PathBuf,
    pub policy_json: PathBuf,
    pub design_directory: PathBuf,
}

#[derive(Debug, Error)]
pub enum OverlayConfigError {
    #[error("overlay config directory could not be created: {0}")]
    Directory(#[source] std::io::Error),
    #[error("overlay config could not be written: {0}")]
    Write(#[source] std::io::Error),
    #[error("overlay config could not be serialized: {0}")]
    Serialize(#[source] serde_json::Error),
}

pub fn initialize_overlay_files(app_dir: &Path) -> Result<OverlayPaths, OverlayConfigError> {
    fs::create_dir_all(app_dir).map_err(OverlayConfigError::Directory)?;
    let theme_json = app_dir.join("overlay-theme.json");
    let labels_json = app_dir.join("overlay_key_labels.json");
    let policy_json = app_dir.join("overlay_key_policy.json");

    write_json_if_missing(&theme_json, &OverlayConfig::default())?;
    write_json_if_missing(&labels_json, &OverlayKeyLabels::default())?;
    write_text_if_missing(
        &policy_json,
        "{\n  \"version\": 2,\n  \"default_single_key\": \"continue_display\",\n  \"default_shortcut\": \"record_and_display_break\",\n  \"overrides\": {}\n}\n",
    )?;

    let config = load_theme(&theme_json);
    let design_directory = app_dir.join(&config.design_directory);
    fs::create_dir_all(&design_directory).map_err(OverlayConfigError::Directory)?;
    write_text_if_missing(&design_directory.join("key.svg"), DEFAULT_KEY_SVG)?;
    write_text_if_missing(&design_directory.join("modifier.svg"), DEFAULT_MODIFIER_SVG)?;
    write_text_if_missing(&design_directory.join("mouse.svg"), DEFAULT_MOUSE_SVG)?;

    Ok(OverlayPaths {
        theme_json,
        labels_json,
        policy_json,
        design_directory,
    })
}

/// JSONを読み込み、必要なら旧versionからv3へ移行する。破損ファイルは保持する。
pub fn load_theme(path: &Path) -> OverlayConfig {
    let default = OverlayConfig::default();
    let Ok(text) = fs::read_to_string(path) else {
        return default;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return default;
    };

    let version = value.get("version").and_then(Value::as_u64).unwrap_or(0);
    let has_v1_fields = value.pointer("/position").is_some()
        || value.pointer("/animation/hold_ms").is_some()
        || value.pointer("/stack/max_items").is_some();
    let is_v1 = version < 2 && (version == 1 || has_v1_fields);
    let needs_rewrite = is_v1 || version < 3;
    let config = if is_v1 {
        migrate_v1_to_v2(&value)
    } else {
        serde_json::from_value::<OverlayConfig>(value).ok()
    };
    let Some(config) = config else {
        return default;
    };
    let config = config.sanitize();
    if needs_rewrite {
        let _ = save_theme_atomic(path, &config);
    }
    config
}

pub fn save_theme_atomic(path: &Path, config: &OverlayConfig) -> Result<(), OverlayConfigError> {
    let text = serde_json::to_string_pretty(&config.clone().sanitize())
        .map_err(OverlayConfigError::Serialize)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("overlay-theme.json");
    let temp = path.with_file_name(format!("{name}.{stamp}.tmp"));
    let result = (|| {
        let mut file = fs::File::create(&temp).map_err(OverlayConfigError::Write)?;
        file.write_all(format!("{text}\n").as_bytes())
            .map_err(OverlayConfigError::Write)?;
        file.flush().map_err(OverlayConfigError::Write)?;
        file.sync_all().map_err(OverlayConfigError::Write)?;
        if fs::rename(&temp, path).is_err() {
            if path.exists() {
                fs::remove_file(path).map_err(OverlayConfigError::Write)?;
            }
            fs::rename(&temp, path).map_err(OverlayConfigError::Write)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn migrate_v1_to_v2(value: &Value) -> Option<OverlayConfig> {
    let mut config = OverlayConfig::default();
    config.margin_px = value
        .pointer("/position/bottom_margin")
        .and_then(Value::as_i64)
        .unwrap_or(i64::from(config.margin_px)) as i32;
    config.item_gap_px = value
        .pointer("/position/item_gap")
        .and_then(Value::as_i64)
        .unwrap_or(i64::from(config.item_gap_px)) as i32;
    config.display_duration_ms = value
        .pointer("/animation/hold_ms")
        .and_then(Value::as_u64)
        .unwrap_or(config.display_duration_ms);
    config.animation.enter_ms = value
        .pointer("/animation/slide_in_ms")
        .and_then(Value::as_u64)
        .unwrap_or(config.animation.enter_ms);
    config.animation.exit_ms = value
        .pointer("/animation/fade_out_ms")
        .and_then(Value::as_u64)
        .unwrap_or(config.animation.exit_ms);
    config.animation.slide_distance = value
        .pointer("/animation/slide_distance")
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(config.animation.slide_distance);
    config.animation.start_scale = value
        .pointer("/animation/start_scale")
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .unwrap_or(config.animation.start_scale);
    config.max_rows = value
        .pointer("/stack/max_items")
        .and_then(Value::as_u64)
        .unwrap_or(config.max_rows as u64) as usize;
    Some(config.sanitize())
}

fn write_json_if_missing<T: Serialize>(path: &Path, value: &T) -> Result<(), OverlayConfigError> {
    if path.exists() {
        return Ok(());
    }
    let text = serde_json::to_string_pretty(value).map_err(OverlayConfigError::Serialize)?;
    fs::write(path, format!("{text}\n")).map_err(OverlayConfigError::Write)
}

fn write_text_if_missing(path: &Path, text: &str) -> Result<(), OverlayConfigError> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, text).map_err(OverlayConfigError::Write)
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn default_enabled() -> bool {
    true
}

fn default_design_directory() -> String {
    "overlay/default".to_owned()
}

fn default_display_duration_ms() -> u64 {
    1840
}

fn default_margin_px() -> i32 {
    64
}

fn default_item_gap_px() -> i32 {
    10
}

fn default_max_rows() -> usize {
    3
}

fn default_cache_items() -> usize {
    128
}

fn default_max_window_width() -> u32 {
    16_000
}

fn default_allowed_positions() -> Vec<OverlayPosition> {
    // Active-window avoidance starts with the two least intrusive bottom corners.
    // Users can still opt into any of the other positions from the settings UI.
    vec![OverlayPosition::BottomLeft, OverlayPosition::BottomRight]
}

pub fn load_labels(path: &Path) -> OverlayKeyLabels {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<OverlayKeyLabels>(&text).ok())
        .map(OverlayKeyLabels::merge_with_defaults)
        .unwrap_or_default()
}

pub fn save_labels_atomic(
    path: &Path,
    labels: &OverlayKeyLabels,
) -> Result<(), OverlayConfigError> {
    let text = serde_json::to_string_pretty(labels).map_err(OverlayConfigError::Serialize)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, format!("{text}\n")).map_err(OverlayConfigError::Write)?;
    if fs::rename(&temp, path).is_err() {
        if path.exists() {
            fs::remove_file(path).map_err(OverlayConfigError::Write)?;
        }
        fs::rename(&temp, path).map_err(OverlayConfigError::Write)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_overlay_config_uses_bottom_corner_avoidance_positions() {
        let config = OverlayConfig::default();
        assert_eq!(config.display_duration_ms, 1840);
        assert_eq!(config.max_rows, 3);
        assert_eq!(config.fixed_position, OverlayPosition::BottomCenter);
        assert_eq!(
            config.allowed_positions,
            vec![OverlayPosition::BottomLeft, OverlayPosition::BottomRight]
        );
        assert_eq!(config.display_mode, OverlayDisplayMode::Desktop);
        assert_eq!(config.monitor_mode, OverlayMonitorMode::ActiveWindow);
        assert!(config.fixed_monitor_id.is_empty());
        assert_eq!(config.animation.enter_ms, 160);
        assert_eq!(config.animation.exit_ms, 400);
    }

    #[test]
    fn sanitize_preserves_saved_position_order_while_removing_duplicates() {
        let config = OverlayConfig {
            allowed_positions: vec![
                OverlayPosition::TopRight,
                OverlayPosition::BottomLeft,
                OverlayPosition::TopRight,
            ],
            ..OverlayConfig::default()
        }
        .sanitize();

        assert_eq!(
            config.allowed_positions,
            vec![OverlayPosition::TopRight, OverlayPosition::BottomLeft]
        );
    }

    #[test]
    fn japanese109_labels_use_keyboard_specific_names() {
        let labels = OverlayKeyLabels::default();
        assert_eq!(labels.keyboard_layout, KeyboardLayout::Japanese109);
        assert_eq!(labels.display("Cancel"), "Break");
        assert_eq!(labels.display("Equal"), "セミコロン");
        assert_eq!(labels.display("Backquote"), "＠");
        assert_eq!(labels.display("OemAuto"), "半角／全角");
        assert_eq!(labels.display("Kanji"), "Alt+半角／全角");
    }

    #[test]
    fn switching_to_us_layout_preserves_custom_labels_and_reverts_defaults() {
        let mut labels = OverlayKeyLabels::default();
        labels.keys.insert("Cancel".to_owned(), "中断".to_owned());
        let labels = labels.with_layout(KeyboardLayout::Us104);
        assert_eq!(labels.keyboard_layout, KeyboardLayout::Us104);
        assert_eq!(labels.display("Cancel"), "中断");
        assert_eq!(labels.display("Equal"), "Equal");
        let labels = labels.with_layout(KeyboardLayout::Japanese109);
        assert_eq!(labels.display("Cancel"), "中断");
        assert_eq!(labels.display("Equal"), "セミコロン");
    }

    #[test]
    fn sanitize_clamps_out_of_range_values() {
        let config = OverlayConfig {
            display_duration_ms: 99_999,
            max_rows: 99,
            background: OverlayBackgroundConfig {
                opacity: 2.0,
                ..OverlayBackgroundConfig::default()
            },
            ..OverlayConfig::default()
        };
        let config = config.sanitize();
        assert_eq!(config.display_duration_ms, 10_000);
        assert_eq!(config.max_rows, 10);
        assert_eq!(config.background.opacity, 1.0);
    }

    #[test]
    fn reset_appearance_preserves_position_and_operational_fields() {
        let mut config = OverlayConfig::default();
        config.enabled = false;
        config.display_mode = OverlayDisplayMode::ActiveWindow;
        config.monitor_mode = OverlayMonitorMode::Fixed;
        config.fixed_monitor_id = "monitor-2".to_owned();
        config.position_mode = OverlayPositionMode::Auto;
        config.fixed_position = OverlayPosition::TopLeft;
        config.allowed_positions = vec![OverlayPosition::TopLeft, OverlayPosition::BottomRight];
        config.preferred_position = OverlayPosition::TopLeft;
        config.design_directory = "custom/design".to_owned();
        config.display_duration_ms = 5_000;
        config.margin_px = 99;
        config.max_rows = 9;
        config.background.color = "#ABCDEF".to_owned();
        config.animation.enter = OverlayInAnimation::Pop;

        let reset = config.clone().reset_appearance_from_defaults();

        assert!(!reset.enabled);
        assert_eq!(reset.display_mode, config.display_mode);
        assert_eq!(reset.monitor_mode, config.monitor_mode);
        assert_eq!(reset.fixed_monitor_id, config.fixed_monitor_id);
        assert_eq!(reset.position_mode, config.position_mode);
        assert_eq!(reset.fixed_position, config.fixed_position);
        assert_eq!(reset.allowed_positions, config.allowed_positions);
        assert_eq!(reset.preferred_position, config.preferred_position);
        assert_eq!(reset.design_directory, OverlayConfig::default().design_directory);
        assert_eq!(reset.display_duration_ms, OverlayConfig::default().display_duration_ms);
        assert_eq!(reset.max_rows, OverlayConfig::default().max_rows);
        assert_eq!(reset.background.color, OverlayConfig::default().background.color);
        assert_eq!(reset.animation.enter, OverlayConfig::default().animation.enter);
    }

    #[test]
    fn v1_theme_migrates_to_v3_without_changing_defaults() {
        let value = serde_json::json!({
            "version": 1,
            "position": { "bottom_margin": 72, "item_gap": 12 },
            "animation": { "slide_in_ms": 120, "hold_ms": 1500, "fade_out_ms": 300, "slide_distance": 20, "start_scale": 0.9 },
            "stack": { "max_items": 5 }
        });
        let config = migrate_v1_to_v2(&value).expect("migration");
        assert_eq!(config.version, 3);
        assert_eq!(config.margin_px, 72);
        assert_eq!(config.display_duration_ms, 1500);
        assert_eq!(config.max_rows, 5);
    }

    #[test]
    fn load_theme_writes_v2_after_successful_v1_migration() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("shortcut-overlay-migration-{unique}"));
        fs::create_dir_all(&directory).expect("create directory");
        let path = directory.join("overlay-theme.json");
        fs::write(
            &path,
            r#"{
              "version": 1,
              "position": {"bottom_margin": 80, "item_gap": 8},
              "animation": {"slide_in_ms": 120, "hold_ms": 900, "fade_out_ms": 250},
              "stack": {"max_items": 4}
            }"#,
        )
        .expect("write v1 theme");

        let config = load_theme(&path);
        let migrated_text = fs::read_to_string(&path).expect("read migrated theme");
        let migrated: Value = serde_json::from_str(&migrated_text).expect("parse migrated theme");
        assert_eq!(config.version, 3);
        assert_eq!(config.display_duration_ms, 900);
        assert_eq!(migrated.get("version").and_then(Value::as_u64), Some(3));
        assert!(migrated.get("display_duration_ms").is_some());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn corrupt_v2_theme_falls_back_without_overwriting_user_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("shortcut-overlay-corrupt-{unique}"));
        fs::create_dir_all(&directory).expect("create directory");
        let path = directory.join("overlay-theme.json");
        let original = r#"{"version":2,"display_duration_ms":"broken"}"#;
        fs::write(&path, original).expect("write corrupt theme");

        let config = load_theme(&path);
        assert_eq!(config.version, 3);
        assert_eq!(config.display_duration_ms, 1840);
        assert_eq!(config.max_rows, 3);
        assert_eq!(fs::read_to_string(&path).expect("read theme"), original);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn invalid_background_color_falls_back() {
        let mut config = OverlayConfig::default();
        config.background.color = "red".to_owned();
        assert_eq!(config.sanitize().background.color, DEFAULT_BACKGROUND);
    }

    #[test]
    fn initialization_creates_editable_files_and_preserves_existing_json() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("shortcut-overlay-config-{unique}"));
        let paths = initialize_overlay_files(&directory).expect("initialize overlay files");
        assert!(paths.theme_json.exists());
        assert!(paths.labels_json.exists());
        assert!(paths.policy_json.exists());
        assert!(paths.design_directory.join("key.svg").exists());
        fs::write(&paths.labels_json, "{ invalid json").expect("write invalid user json");
        initialize_overlay_files(&directory).expect("second initialization");
        assert_eq!(
            fs::read_to_string(&paths.labels_json).expect("read labels"),
            "{ invalid json"
        );
        let labels = load_labels(&paths.labels_json);
        assert_eq!(labels.display("Ctrl"), "Ctrl");
        let _ = fs::remove_dir_all(directory);
    }
}
