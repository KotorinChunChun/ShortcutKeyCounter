# ShortcutKeyCounter v0.1.2 Overlay拡張
# Luna向けタスク実装パッケージ

作成日: 2026-08-16  
対象: ShortcutKeyCounter v0.1.1 Overlay実装済みソース  
目的: v0.1.2でオーバーレイ設定・通常キー表示・表示/記録ポリシー・アニメーション等を追加する。

---

## 0. このパッケージの使い方

Lunaへは、**原則1回につき1タスクMDだけ渡す**。

管理側は以下の順番で実行する。

```text
OVL-01 設定モデル拡張
  ↓
OVL-02 表示時間
OVL-03 9位置
OVL-05 背景色
OVL-06 単独キーOverlayEvent
OVL-09 IN/OUTアニメーション
OVL-10 最大行数
  ↓
OVL-04 自動位置
OVL-07 表示/記録ポリシー
OVL-08 単独キー横連結
OVL-11 クイックOFF
  ↓
OVL-12 設定画面/UI/API
  ↓
OVL-13 移行・結合テスト
```

並行化する場合:

```text
Wave A: OVL-01

Wave B:
  OVL-02
  OVL-03
  OVL-05
  OVL-06
  OVL-09
  OVL-10

Wave C:
  OVL-04
  OVL-07
  OVL-08
  OVL-11

Wave D:
  OVL-12

Wave E:
  OVL-13
```

---

# 1. 現行v0.1.1で壊してはいけないもの

現在のOverlayは次を前提とする。

- Rust + Win32 Native Layered Window
- Overlay専用WebViewは作らない
- `WS_EX_LAYERED / WS_EX_TRANSPARENT / WS_EX_TOOLWINDOW / WS_EX_NOACTIVATE / WS_EX_TOPMOST`
- `WM_NCHITTEST -> HTTRANSPARENT`
- Foreground Windowのあるモニターへ表示
- Per-monitor DPI
- Overlay Windowは1個を再利用
- bounded channel / nonblocking `try_send`
- Local/ElevatedのOverlay専用50ms dedup
- DB用1秒DuplicateGuardとは分離
- SVGと最終ショートカット描画のcache
- `overlay_key_labels.json`
- `overlay-theme.json`
- Overlay障害がCapture/DBへ波及しない
- idle時busy loop禁止

これらを維持すること。

---

# 2. 現行コードの探索

ソースの細かなファイル名はパッケージ世代によって差があり得るため、変更前に必ず検索する。

PowerShell例:

```powershell
rg -n "OverlayEvent|OverlayItem|OverlayPhase|try_show|try_send_elevated" .
rg -n "overlay-theme|overlay_enabled|hold_ms|fade_out_ms|slide_in_ms" .
rg -n "WS_EX_LAYERED|HTTRANSPARENT|MonitorFromWindow|rcWork" .
rg -n "DuplicateGuard|is_recordable|normalize|excluded" apps crates
rg -n "get_settings|set_.*settings|SettingsSnapshot" apps
```

想定モジュール:

```text
apps/shortcut-key-counter/src/overlay/
  mod.rs
  manager.rs
  event.rs
  state.rs
  animation.rs
  renderer.rs
  config.rs
  key_labels.rs
  ...

crates/shortcut-windows/src/
  overlay_window.rs
  monitor.rs

apps/shortcut-key-counter/ui/
  index.html
  src/
  styles/
```

実際のファイル名が異なる場合、**役割が同じ既存ファイルへ統合する**。
勝手に似たモジュールを二重作成しない。

---

# 3. v0.1.2のキー処理原則

v0.1.1では主に「記録対象ショートカット → Overlay通知 → DB」という流れだった。

v0.1.2では、表示と記録を分離する。

```text
Raw input
   ↓
normalize / foreground / exclusion
   ↓
KeyPolicyResolver
   ├─ Ignore
   │    └─ 何もしない
   │
   ├─ DisplayOnly
   │    └─ Overlayへ送る / DBへ送らない
   │
   └─ DisplayAndRecord
        ├─ Overlayへ送る
        └─ DB DuplicateGuard → SQLite
```

### 絶対条件

`DisplayOnly` の通常キーが、

- `shortcut_events`
- `shortcut_daily`
- ranking
- application stats
- realtimeの「記録成功」系集計

へ混入してはいけない。

---

# 4. 共通禁止事項

Lunaは各タスクで次を行わない。

- 要求外の大規模リファクタリング
- Hook callback内で描画
- Hook callback内でJSON/SVG/DBファイルI/O
- WebView Overlayへ変更
- 常時60FPSループ
- `unwrap()` / `expect()` を新規の通常実行経路へ乱用
- DB内部shortcut名の表示名への置換
- DB 1秒DuplicateGuardの意味変更
- Local/Elevated 50ms dedupの削除
- 除外アプリ上でOverlayを表示
- Window focusを奪う変更
- click-throughを壊す変更

---

# 5. 共通テストコマンド

各タスク完了後:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo test --workspace --all-features --offline
```

Frontend変更がある場合:

```powershell
cd apps\shortcut-key-counter\ui
npm run build
```

既存build scriptがある場合は最終的に:

```powershell
.\scripts\build-release.ps1
```

---

# 6. Luna完了報告フォーマット

各タスク終了時に必ず次を返す。

```text
Task:
Status: PASS / BLOCKED

Changed files:
- ...

Implemented:
- ...

Tests:
- command
- result

Regression checks:
- DB shortcut identity unchanged: YES/NO
- DB DuplicateGuard unchanged: YES/NO
- Local/Elevated dedup unchanged: YES/NO
- excluded app behavior unchanged: YES/NO
- overlay click-through/focus design unchanged: YES/NO

Integration requests:
- none
  または
- ...

Remaining concerns:
- none
  または
- ...
```

---

# 7. タスク一覧

| ID | 内容 | 依存 |
|---|---|---|
| OVL-01 | Overlay設定モデル拡張 | - |
| OVL-02 | 表示時間を設定可能化 | OVL-01 |
| OVL-03 | 表示位置9箇所 | OVL-01 |
| OVL-04 | 自動位置ローテーション | OVL-03 |
| OVL-05 | 背景色・透明度 | OVL-01 |
| OVL-06 | 通常単独キーをOverlayEvent化 | OVL-01 |
| OVL-07 | Ignore / DisplayOnly / DisplayAndRecord | OVL-06 |
| OVL-08 | 単独キーを同一行へ横連結 | OVL-06, OVL-07 |
| OVL-09 | IN/OUTアニメーション選択 | OVL-01 |
| OVL-10 | 最大行数可変 | OVL-01 |
| OVL-11 | ワンクリックOverlay OFF | OVL-01 |
| OVL-12 | 設定画面 + Tauri API統合 | OVL-02〜11 |
| OVL-13 | 設定移行・結合・回帰試験 | OVL-12 |
