# OVL-13 設定移行・結合テスト・回帰Gate

## 目的

v0.1.1利用者の既存設定を壊さずv0.1.2へ移行し、今回の全機能を結合確認する。

---

## 依存

OVL-12

---

# 1. Source of Truth整理

最終的に次を明確にする。

```text
SQLite settings.overlay_enabled
  → Overlay機能ON/OFF

overlay-theme.json
  → 表示時間 / 位置 / 背景 / 行数 / animation

overlay_key_labels.json
  → 表示文字列のみ

overlay_key_policy.json
  → Ignore / DisplayOnly / DisplayAndRecord
```

`enabled`をJSONとSQLiteの両方に保存しない。

---

# 2. overlay-theme v1 -> v2

旧例:

```json
{
  "version": 1,
  "position": {
    "bottom_margin": 64,
    "item_gap": 10
  },
  "animation": {
    "slide_in_ms": 160,
    "hold_ms": 840,
    "fade_out_ms": 400,
    "slide_distance": 18,
    "start_scale": 0.96
  },
  "stack": {
    "max_items": 3
  }
}
```

新例:

```json
{
  "version": 2,
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

## Migration関数

```rust
#[derive(Deserialize)]
struct OverlayThemeV1 {
    position: PositionV1,
    animation: AnimationV1,
    stack: StackV1,
}

fn migrate_v1_to_v2(old: OverlayThemeV1) -> OverlayConfig {
    OverlayConfig {
        version: 2,
        enabled: true, // 最終的にSQLite側なら削除
        display_duration_ms: old.animation.hold_ms,
        position_mode: OverlayPositionMode::Fixed,
        fixed_position: OverlayPosition::BottomCenter,
        margin_px: old.position.bottom_margin,
        item_gap_px: old.position.item_gap,
        max_rows: old.stack.max_items,
        background: OverlayBackgroundConfig {
            color: "#202020".into(),
            opacity: 0.8,
        },
        animation: OverlayAnimationConfig {
            enter: OverlayInAnimation::SlideUp,
            exit: OverlayOutAnimation::Fade,
            enter_ms: old.animation.slide_in_ms,
            exit_ms: old.animation.fade_out_ms,
            slide_distance: old.animation.slide_distance,
            start_scale: old.animation.start_scale,
        },
    }.sanitize()
}
```

---

## ユーザーファイルを勝手に破壊しない

現行方針を維持:

```text
JSON破損
→ default fallback
→ Overlayだけ継続/無効化
→ Capture/DB継続
→ 元ユーザーファイルを無断削除しない
```

Migration成功時の書き戻しは、

1. temp file
2. flush
3. rename/replace

の原子的保存を推奨。

---

# 3. KeyPolicy初回生成

既存ユーザーにpolicy fileがない場合:

```text
default_single_key = DisplayOnly
default_shortcut = DisplayAndRecord
overrides = empty
```

これで従来shortcut記録はそのまま。

---

# 4. 結合テストマトリクス

## 4.1 表示時間

```text
0.1s
0.5s
1.5s
3s
10s
```

- Enter
- Stable
- Exit
- Remove

が設定通り。

---

## 4.2 9位置

各位置を実機で確認。

```text
TopLeft
TopCenter
TopRight
MiddleLeft
Center
MiddleRight
BottomLeft
BottomCenter
BottomRight
```

- taskbarを避ける
- 負座標monitor
- 100% / 125% / 150% DPI
- monitor切替

---

## 4.3 auto position

- セッション中固定
- 消滅後次位置
- 9回で循環
- monitor変更時も区分維持

---

## 4.4 背景

```text
#202020 80%
#FFFFFF 60%
#000000 0%
#FF0000 100%
invalid
```

invalidはfallback。

---

## 4.5 単独キー

```text
A
B
C
1
Enter
F5
ArrowLeft
Backspace
Space
```

Overlay表示、DB増加なし。

---

## 4.6 横連結

```text
A B C
```

期待:

```text
[A][B][C]
```

```text
A B Ctrl+C D E
```

期待:

```text
[A][B]
[Ctrl]+[C]
[D][E]
```

---

## 4.7 Policy

```text
A = Ignore
B = DisplayOnly
Ctrl+C = DisplayAndRecord
```

期待:

```text
A: display 0 / DB 0
B: display 1 / DB 0
Ctrl+C: display 1 / DB 1
```

---

## 4.8 animation

全IN × 代表OUTを確認。

最低限:

```text
none / none
fade / fade
slide-up / fade
slide-left / slide-right
pop / shrink
```

---

## 4.9 max rows

```text
1
3
5
10
```

4件以上のshortcutとsingle sequenceを混ぜる。

---

## 4.10 quick OFF

Overlay OFF時:

- DB shortcut記録は継続
- DisplayOnly単独キーはDBへ行かない
- Overlay Windowはhide
- animation timer停止

---

# 5. 回帰試験

## Capture

- normal app
- elevated app
- UAC cancel
- helper reconnect
- Local/Elevated二重取得なし

## Exclusion

除外アプリ上:

```text
shortcut
single key
```

両方Overlayなし、DBなし。

## DB

既存fixtureで:

- shortcut_events
- shortcut_daily
- ranking
- app stats

の結果が通常キー表示追加によって変わらない。

## Window

- focusを奪わない
- Alt+Tabに出ない
- taskbarに出ない
- mouse click-through
- 1 HWND再利用

## Performance

idle:

```text
items=0
→ timer stopped
→ no busy loop
```

rapid input:

```text
10 keys/sec × 60 sec
```

- Hook stallなし
- queue overflowを診断可能
- memory増え続けない
- cache上限維持

---

# 6. build-release.ps1 Gate追加候補

静的検索だけに依存しすぎないが、最低限の回帰チェックは有効。

例:

```powershell
Assert-SourceContains "display_duration_ms"
Assert-SourceContains "OverlayPositionMode"
Assert-SourceContains "DisplayOnly"
Assert-SourceContains "max_rows"
Assert-SourceContains "overlay-enabled-changed"
Assert-SourceDoesNotContain "display=0 record=1"
```

既存Gate:

- 50ms dedup
- Overlay通知とDB DuplicateGuardの順序
- Win32 styles
- WebView依存混入禁止

を削除しない。

---

# 7. 最終完了条件

- 全13タスク完了
- fmt PASS
- clippy PASS
- cargo test PASS
- frontend build PASS
- release build PASS
- Windows実機9位置 PASS
- multi-monitor/DPI PASS
- elevated PASS
- quick OFF PASS
- DB回帰 PASS
- idle負荷 PASS

---

# 8. 最終報告テンプレート

```text
ShortcutKeyCounter v0.1.2 Overlay extension

Build:
- cargo fmt:
- clippy:
- cargo test:
- npm build:
- build-release.ps1:

Features:
- display duration:
- 9 positions:
- auto position:
- background:
- single keys:
- key policy:
- horizontal sequence:
- animations:
- max rows:
- quick toggle:
- settings UI:

Regression:
- DB:
- Local/Elevated dedup:
- excluded apps:
- focus/click-through:
- multi-monitor/DPI:
- idle CPU/memory:

Known issues:
- none / ...
```
