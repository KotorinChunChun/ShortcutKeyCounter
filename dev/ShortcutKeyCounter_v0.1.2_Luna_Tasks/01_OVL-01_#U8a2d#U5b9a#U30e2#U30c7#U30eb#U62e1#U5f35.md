# OVL-01 Overlay設定モデル拡張

## 目的

v0.1.2で増える設定を、一か所に型付きで保持できるようにする。

このタスクでは**表示挙動そのものは変更しない**。

---

## 依存

なし。

---

## 最初に探すコード

```powershell
rg -n "overlay-theme|OverlayTheme|OverlayConfig|hold_ms|slide_in_ms|fade_out_ms" apps
rg -n "overlay_enabled|get_settings|SettingsSnapshot" apps crates
```

---

## 変更対象候補

```text
apps/shortcut-key-counter/src/overlay/config.rs
apps/shortcut-key-counter/src/overlay/mod.rs
resources/overlay/default/overlay-theme.json
```

実際の配置に合わせる。

---

## 新しい設定モデル

推奨:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPositionMode {
    Fixed,
    Auto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayInAnimation {
    None,
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Pop,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayOutAnimation {
    None,
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Shrink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayBackgroundConfig {
    pub color: String,
    pub opacity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAnimationConfig {
    pub enter: OverlayInAnimation,
    pub exit: OverlayOutAnimation,
    pub enter_ms: u64,
    pub exit_ms: u64,
    pub slide_distance: f32,
    pub start_scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayConfig {
    pub version: u32,
    pub enabled: bool,
    pub display_duration_ms: u64,
    pub position_mode: OverlayPositionMode,
    pub fixed_position: OverlayPosition,
    pub margin_px: i32,
    pub item_gap_px: i32,
    pub max_rows: usize,
    pub background: OverlayBackgroundConfig,
    pub animation: OverlayAnimationConfig,
}
```

### 既定値

```rust
impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            version: 2,
            enabled: true,
            display_duration_ms: 840,
            position_mode: OverlayPositionMode::Fixed,
            fixed_position: OverlayPosition::BottomCenter,
            margin_px: 64,
            item_gap_px: 10,
            max_rows: 3,
            background: OverlayBackgroundConfig {
                color: "#202020".to_string(),
                opacity: 0.80,
            },
            animation: OverlayAnimationConfig {
                enter: OverlayInAnimation::SlideUp,
                exit: OverlayOutAnimation::Fade,
                enter_ms: 160,
                exit_ms: 400,
                slide_distance: 18.0,
                start_scale: 0.96,
            },
        }
    }
}
```

---

## JSON例

`overlay-theme.json` をversion 2へ拡張する。

```json
{
  "version": 2,
  "enabled": true,
  "display_duration_ms": 840,
  "position_mode": "fixed",
  "fixed_position": "bottom-center",
  "margin_px": 64,
  "item_gap_px": 10,
  "max_rows": 3,
  "background": {
    "color": "#202020",
    "opacity": 0.8
  },
  "animation": {
    "enter": "slide-up",
    "exit": "fade",
    "enter_ms": 160,
    "exit_ms": 400,
    "slide_distance": 18,
    "start_scale": 0.96
  }
}
```

---

## Validation

読み込み直後に値を正規化する。

```rust
impl OverlayConfig {
    pub fn sanitize(mut self) -> Self {
        self.display_duration_ms = self.display_duration_ms.clamp(100, 10_000);
        self.max_rows = self.max_rows.clamp(1, 10);
        self.margin_px = self.margin_px.clamp(0, 500);

        self.background.opacity = self.background.opacity.clamp(0.0, 1.0);

        self.animation.enter_ms = self.animation.enter_ms.clamp(0, 2_000);
        self.animation.exit_ms = self.animation.exit_ms.clamp(0, 2_000);
        self.animation.slide_distance = self.animation.slide_distance.clamp(0.0, 200.0);
        self.animation.start_scale = self.animation.start_scale.clamp(0.5, 1.5);

        if !is_hex_color(&self.background.color) {
            self.background.color = "#202020".to_string();
        }

        self
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}
```

---

## 旧JSONとの互換

旧 `position.bottom_margin`, `animation.hold_ms`, `stack.max_items` がある場合は、
OVL-13で正式なmigrationを入れる。

このタスクでは少なくともJSON破損時のdefault fallbackを壊さない。

---

## Unit Testサンプル

```rust
#[test]
fn default_overlay_config_matches_v011_behavior() {
    let cfg = OverlayConfig::default();

    assert_eq!(cfg.display_duration_ms, 840);
    assert_eq!(cfg.max_rows, 3);
    assert_eq!(cfg.fixed_position, OverlayPosition::BottomCenter);
    assert_eq!(cfg.animation.enter_ms, 160);
    assert_eq!(cfg.animation.exit_ms, 400);
}

#[test]
fn sanitize_clamps_out_of_range_values() {
    let mut cfg = OverlayConfig::default();
    cfg.display_duration_ms = 99_999;
    cfg.max_rows = 99;
    cfg.background.opacity = 2.0;

    let cfg = cfg.sanitize();

    assert_eq!(cfg.display_duration_ms, 10_000);
    assert_eq!(cfg.max_rows, 10);
    assert_eq!(cfg.background.opacity, 1.0);
}
```

---

## 完了条件

- 新設定型がSerialize/Deserialize可能
- defaultがv0.1.1相当の見た目・時間
- 不正値でpanicしない
- JSON破損fallbackを維持
- このタスク単体では表示挙動に変更なし

---

## 禁止

- DB schemaをこのタスクで変更しない
- Overlay Managerロジックを変更しない
- UIを変更しない
- default表示位置を変更しない
