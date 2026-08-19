# OVL-12 設定画面 + Tauri API統合

## 目的

OVL-02〜11の設定をGUIから変更できるようにする。

---

## 依存

OVL-02〜OVL-11

---

## UI構成

```text
オーバーレイ
────────────────────────

[✓] オーバーレイを表示

表示
  表示時間        [ 1.5 ] 秒
  最大表示行数    [ 3 ]

位置
  (●) 固定
  ( ) 自動

  [左上] [上中央] [右上]
  [左中] [中央  ] [右中]
  [左下] [下中央] [右下]

外観
  背景色          [#202020] [color picker]
  透明度          [80%]

アニメーション
  IN              [スライドアップ ▼]
  OUT             [フェード ▼]
  IN時間           [160 ms]
  OUT時間          [400 ms]

キー
  既定の通常キー   [表示のみ ▼]
  [キー表示・記録設定...]
```

---

## Backend DTO

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlaySettingsDto {
    pub enabled: bool,
    pub display_duration_ms: u64,
    pub position_mode: OverlayPositionMode,
    pub fixed_position: OverlayPosition,
    pub max_rows: usize,
    pub background_color: String,
    pub background_opacity: f32,
    pub enter_animation: OverlayInAnimation,
    pub exit_animation: OverlayOutAnimation,
    pub enter_ms: u64,
    pub exit_ms: u64,
    pub default_single_key_action: KeyAction,
}
```

---

## Tauri commands

```rust
#[tauri::command]
pub fn get_overlay_settings(
    state: tauri::State<'_, AppState>,
) -> Result<OverlaySettingsDto, CommandError> {
    state.overlay.settings_snapshot()
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn set_overlay_settings(
    state: tauri::State<'_, AppState>,
    settings: OverlaySettingsDto,
) -> Result<OverlaySettingsDto, CommandError> {
    let sanitized = settings.sanitize()?;
    state.overlay.apply_settings(sanitized.clone())?;
    state.settings_store.persist_overlay(&sanitized)?;
    Ok(sanitized)
}
```

### 原則

```text
validate
↓
runtime適用
↓
永続化
```

または、永続化失敗時のrollbackを明確にする。

既存設定保存パターンへ合わせる。

---

## TypeScript型

```ts
export type OverlayPosition =
  | "top-left"
  | "top-center"
  | "top-right"
  | "middle-left"
  | "center"
  | "middle-right"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right";

export type OverlayPositionMode = "fixed" | "auto";

export type OverlayInAnimation =
  | "none"
  | "fade"
  | "slide-up"
  | "slide-down"
  | "slide-left"
  | "slide-right"
  | "pop";

export type OverlayOutAnimation =
  | "none"
  | "fade"
  | "slide-up"
  | "slide-down"
  | "slide-left"
  | "slide-right"
  | "shrink";

export type KeyAction =
  | "ignore"
  | "display_only"
  | "display_and_record";

export interface OverlaySettings {
  enabled: boolean;
  display_duration_ms: number;
  position_mode: OverlayPositionMode;
  fixed_position: OverlayPosition;
  max_rows: number;
  background_color: string;
  background_opacity: number;
  enter_animation: OverlayInAnimation;
  exit_animation: OverlayOutAnimation;
  enter_ms: number;
  exit_ms: number;
  default_single_key_action: KeyAction;
}
```

---

## Frontend API

```ts
import { invoke } from "@tauri-apps/api/core";

export function getOverlaySettings(): Promise<OverlaySettings> {
  return invoke<OverlaySettings>("get_overlay_settings");
}

export function setOverlaySettings(
  settings: OverlaySettings
): Promise<OverlaySettings> {
  return invoke<OverlaySettings>(
    "set_overlay_settings",
    { settings }
  );
}
```

既存のapi.ts wrapperがある場合はそこへ統合。

---

## 9位置selector

HTML例:

```html
<div class="overlay-position-grid" id="overlayPositionGrid">
  <button data-position="top-left">左上</button>
  <button data-position="top-center">上中央</button>
  <button data-position="top-right">右上</button>
  <button data-position="middle-left">左中央</button>
  <button data-position="center">中央</button>
  <button data-position="middle-right">右中央</button>
  <button data-position="bottom-left">左下</button>
  <button data-position="bottom-center">下中央</button>
  <button data-position="bottom-right">右下</button>
</div>
```

CSS:

```css
.overlay-position-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
  max-width: 360px;
}

.overlay-position-grid button[aria-pressed="true"] {
  outline: 2px solid var(--accent);
  background: var(--panel-2);
}
```

auto modeではgridをdisabled風表示にするが、最後のfixed position値は保持する。

---

## 入力validation

Frontendだけに依存しない。

Frontend:

```ts
const durationMs = Math.round(seconds * 1000);

if (durationMs < 100 || durationMs > 10_000) {
  showToast("表示時間は0.1〜10秒で指定してください");
  return;
}
```

Backendでもclamp/reject。

---

## 保存タイミング

推奨:

- toggle/select/buttonは即時保存
- number/color text inputは`change`またはdebounce 300ms

毎`input`でTauri invokeしない。

---

## キーポリシーUI

大量キーを1画面に並べる場合は検索可能にする。

最低限:

```text
キー             表示       記録
A                ON         OFF
B                ON         OFF
Enter            ON         OFF
F5               ON         OFF
Ctrl+C           ON         ON
Ctrl+V           ON         ON
```

内部変換:

```ts
function toKeyAction(display: boolean, record: boolean): KeyAction {
  if (record) return "display_and_record";
  if (display) return "display_only";
  return "ignore";
}
```

記録ON時:

```ts
displayCheckbox.checked = true;
displayCheckbox.disabled = true;
```

または3状態selectにしてもよい。

---

## UI state同期

WebView再生成後:

```text
DOMContentLoaded
↓
get_overlay_settings
↓
render controls
```

tray変更:

```text
overlay-enabled-changed
↓
enabled checkboxだけ同期
```

---

## 完了条件

- 全設定をGUI変更可能
- 保存後即時反映
- 再起動後復元
- auto時9位置ボタン誤操作防止
- UIを閉じてもbackend設定は保持
- invalid値でbackend panicなし
- TypeScript strict build通過

---

## 禁止

- localStorageをSource of Truthにしない
- UIだけでvalidation完結しない
- 設定変更のためOverlay Windowを毎回作り直さない
