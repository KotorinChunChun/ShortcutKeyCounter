# ShortcutKeyCounter v0.1.1
# ショートカットキー・オーバーレイ表示機能 設計書

作成日: 2026-08-16  
対象バージョン: v0.1.1  
対象アプリ: ShortcutKeyCounter

---

## 1. 目的

ShortcutKeyCounter がショートカットキー入力を検知した際に、現在アクティブなウィンドウが存在するモニターの中央下部へ、入力されたショートカットキーを一時的にオーバーレイ表示する。

本機能は以下を重視する。

- PCへの負荷を可能な限り小さくする
- 既存のキーフック・記録処理を遅延させない
- マルチモニター環境で自然に表示する
- キーボード操作やマウス操作を妨げない
- 見栄えのよいアニメーションを行う
- 最大3件まで連続入力を視認できる
- キー外観・表示名を後から容易に変更できる
- DBへ保存する内部キー名には影響を与えない
- 除外アプリ上では表示しない

---

# 2. 採用方式

## 2.1 オーバーレイ実装

オーバーレイ専用のTauri WebViewは作成しない。

**Rust + Win32 Native Layered Window** を採用する。

想定する拡張スタイル:

```text
WS_EX_LAYERED
WS_EX_TRANSPARENT
WS_EX_TOOLWINDOW
WS_EX_NOACTIVATE
```

必要に応じて最前面表示属性を組み合わせる。

この方式により、

- WebView2追加起動が不要
- Alt+Tabに表示されない
- タスクバーに表示されない
- マウス操作を奪わない
- キーボードフォーカスを奪わない
- 背景透過が可能
- アイドル時のCPU使用をほぼゼロにできる

という構成を目指す。

---

# 3. 全体アーキテクチャ

```text
Windows Keyboard / Mouse Hook
            │
            ▼
      Capture Processor
            │
            ▼
   Shortcut Normalizer
            │
            ▼
      Eligibility Check
            │
            ▼
    Excluded App Check
            │
      ┌─────┴─────────────┐
      │                   │
      ▼                   ▼
Overlay Event         DB Recording Path
      │                   │
      ▼                   ▼
Overlay Source       1秒 Duplicate Guard
Deduper                   │
      │                   ▼
      ▼              SQLite Recorder
Overlay Manager
      │
      ▼
Win32 Overlay Window
      │
      ▼
Renderer / Animation
```

---

# 4. 既存Capture処理との分離

Hook callback内では描画処理を絶対に行わない。

Capture側は対象入力を検知した後、Overlay Managerへ軽量イベントを送るだけとする。

例:

```rust
overlay.try_show(shortcut, application);
```

通信はbounded channel等による非同期キューを使用する。

重要事項:

- Hook callbackからWin32描画APIを呼ばない
- Hook callbackからSVGを読み込まない
- Hook callbackからファイルを読み込まない
- Hook callbackからタイマーを生成しない
- Queue満杯時にもCapture処理をブロックしない

---

# 5. 表示対象

## 5.1 表示する入力

ShortcutKeyCounterがショートカット候補として認識した入力を対象とする。

例:

```text
Ctrl+C
Ctrl+V
Ctrl+Shift+P
Alt+Tab
Win+E
Ctrl+MouseLeft
Ctrl+WheelUp
```

通常文字入力は表示対象としない。

---

## 5.2 除外アプリ

除外アプリ上で検知された入力はオーバーレイにも表示しない。

処理順:

```text
入力検知
↓
正規化
↓
Foreground executable取得
↓
除外アプリ判定
↓
除外されていなければOverlayへ送信
```

---

# 6. DB Duplicate Guardとの関係

既存の「同一ショートカットを1秒以内はDBへ重複記録しない」仕様と、オーバーレイ表示は分離する。

例:

```text
00.000 Ctrl+C
00.300 Ctrl+C
```

DB:

```text
Ctrl+C × 1
```

Overlay:

```text
Ctrl+C
Ctrl+C
```

ユーザーが実際に2回入力したことはオーバーレイでは視認できるようにする。

したがって、Overlay EventはDB用1秒Duplicate Guardより前で生成する。

---

# 7. Local / Elevated Captureの重複除去

ShortcutKeyCounterは通常Hookと管理者権限Helperの両方から同じ物理入力を受信する可能性がある。

Overlay専用の短時間Deduperを用意する。

## 判定案

以下がすべて一致した場合、同一物理入力と判断する。

```text
normalized shortcut が同じ
foreground application が同じ
capture source が異なる
受信時刻差が50ms以内
```

この場合は1件だけ表示する。

### 注意

これはDB用1秒Duplicate Guardとは別機能とする。

目的:

- 50ms Deduper:
  Local / Elevatedの二重取得防止

- 1秒 Duplicate Guard:
  DBへの短時間連続記録抑止

---

# 8. 表示位置

## 8.1 対象モニター

現在のForeground Windowが存在するモニターを使用する。

概念:

```text
GetForegroundWindow()
        ↓
MonitorFromWindow()
        ↓
GetMonitorInfo()
        ↓
MONITORINFO.rcWork
```

`rcWork` を利用し、Windowsタスクバー等を除外した作業領域を基準にする。

---

## 8.2 標準表示位置

```text
水平:
作業領域の中央

垂直:
作業領域下端から64px上
```

概念図:

```text
┌──────────────────────────────────┐
│                                  │
│         Active Window            │
│                                  │
│                                  │
│          Ctrl + C                │
│              ↑                   │
│       下端から約64px             │
└──────────────────────────────────┘
```

オーバーレイの実サイズに応じて中央揃えする。

---

# 9. 表示件数

最大3件。

新しい入力を一番下へ追加する。

例:

```text
Ctrl+C
Ctrl+V
Ctrl+S
```

4件目が追加された場合:

```text
Ctrl+C  → 退場
Ctrl+V  → 1段上へ
Ctrl+S  → 1段上へ
Ctrl+Z  → 下から登場
```

常に最新3件を表示する。

---

# 10. 表示時間

## 10.1 採用値

当初案の1秒フェードアウトは長いため、以下へ変更する。

```text
総表示時間:     1.40秒

Slide/Fade In:  0.16秒
安定表示:       0.84秒
Fade Out:       0.40秒
```

タイムライン:

```text
0.00s
│
├─ 0.00 ～ 0.16
│  Slide In + Fade In
│
├─ 0.16 ～ 1.00
│  完全表示
│
├─ 1.00 ～ 1.40
│  Fade Out
│
└─ 1.40
   完全削除
```

### 採用理由

0.4秒のFade Outは、

- 消え方が唐突にならない
- キー入力表示として残像感が長すぎない
- 連続入力時に古い項目が画面へ残りすぎない
- 1.4秒全体で操作フィードバックとして十分視認できる

ため採用する。

---

# 11. アイテム単位TTL

各Overlay Itemは独立した寿命を持つ。

例:

```text
00.0 Ctrl+C
00.4 Ctrl+V
00.8 Ctrl+S
```

消滅:

```text
01.4 Ctrl+C
01.8 Ctrl+V
02.2 Ctrl+S
```

新しい入力が来ても既存アイテムの寿命はリセットしない。

---

# 12. アニメーション

## 12.1 新規アイテム

新しい入力は下からスライドインする。

開始状態:

```text
Y      = +18px
Opacity = 0
Scale   = 0.96
```

0.16秒後:

```text
Y       = 0px
Opacity = 1
Scale   = 1.00
```

イージングは、勢いがありつつ急停止しないものを使用する。

推奨:

```text
ease-out cubic
```

または同等のネイティブ補間。

---

## 12.2 既存アイテム

新しい入力が追加された場合、既存アイテムは上方向へスライドする。

例:

```text
Before:

Ctrl+C
Ctrl+V

After Ctrl+S:

Ctrl+C   ↑
Ctrl+V   ↑
Ctrl+S   ← Slide In
```

移動時間:

```text
160ms
```

---

## 12.3 最古アイテム退場

4件目追加時は最古アイテムを即座に消去するのではなく、短い退場アニメーションを許可する。

推奨:

```text
Opacity 1 → 0
Y 0 → -8px
Duration 120ms
```

ただし、最大描画対象は3件を維持するため、内部的には退場アイテムを別Transient Itemとして扱ってよい。

---

# 13. オーバーレイウィンドウサイズ

固定全画面ウィンドウにはしない。

表示中の最大3項目を収容できる小さなWindowを使用する。

想定最大:

```text
幅:   600px程度
高さ: 240px程度
```

内容に応じて可能な限り小さくする。

理由:

- 描画面積削減
- Layered Window更新コスト削減
- 不要なGPU/CPU処理削減

---

# 14. アイドル時動作

オーバーレイが存在しない場合はアニメーションタイマーを停止する。

```text
Overlay item = 0
↓
Wait状態
↓
CPU ≒ 0
```

キー入力時:

```text
Overlay Event
↓
Overlay Thread起床
↓
Window表示
↓
Animation Timer開始
↓
全Item消滅
↓
Window非表示
↓
Timer停止
↓
Wait
```

常時60FPSで動かし続ける実装は禁止する。

---

# 15. 描画FPS

アニメーション中のみ最大60FPSとする。

ただし、

- 変化がないフレーム
- 完全表示中

では再描画を減らしてよい。

可能であれば次の状態駆動にする。

```text
Entering / Moving / Fading
    → animation timer動作

Stable
    → 再描画不要

No items
    → timer停止
```

---

# 16. 描画デザイン

キー本体はSVGをベースにする。

例:

```text
resources/
└─ overlay/
   └─ default/
      ├─ key.svg
      ├─ modifier.svg
      ├─ mouse.svg
      └─ overlay-theme.json
```

SVG内へキー文字は埋め込まない。

キー形状と文字を分離する。

例:

```text
SVG:
白い立体キートップ

Text:
Ctrl
Shift
C
V
F12
```

これにより、将来デザイン変更が容易になる。

---

# 17. SVGキャッシュ

キー入力ごとにSVGを解析してはいけない。

初期化時またはテーマ変更時:

```text
SVG読込
↓
Parse
↓
Rasterize
↓
ARGB Bitmap Cache
```

表示時:

```text
Bitmap Cache
+
Text
```

のみとする。

---

# 18. ショートカット単位キャッシュ

必要に応じて、最終的なショートカット表示画像もキャッシュする。

例:

```text
HashMap<DisplayShortcut, RenderedBitmap>
```

キャッシュ例:

```text
Ctrl+C
Ctrl+V
Ctrl+Shift+P
Alt+Tab
```

同一ショートカットの2回目以降は再レイアウトを極力避ける。

ただしメモリ上限を設ける。

推奨:

```text
LRU 128件程度
```

---

# 19. キー表示名マッピング

内部キー名とOverlay上の表示名は分離する。

保存場所:

```text
%LOCALAPPDATA%\ShortcutKeyCounter\overlay_key_labels.json
```

例:

```json
{
  "version": 1,
  "keys": {
    "Ctrl": "Ctrl",
    "Alt": "Alt",
    "Win": "Win",
    "Shift": "Shift",

    "Enter": "Enter",
    "Escape": "Esc",
    "Backspace": "Backspace",
    "Space": "Space",

    "MouseLeft": "Left Click",
    "MouseRight": "Right Click",
    "WheelUp": "Wheel ↑",
    "WheelDown": "Wheel ↓"
  }
}
```

---

# 20. キー表示名変更例

例えば、

```json
{
  "Ctrl": "CTRL",
  "Escape": "ESC",
  "MouseLeft": "クリック"
}
```

と変更した場合、

内部値:

```text
Ctrl+C
```

DB:

```text
Ctrl+C
```

Overlay:

```text
CTRL + C
```

となる。

### 重要

以下は絶対に変更しない。

- DBに保存するshortcut
- Duplicate Guardのキー
- Rankingの内部識別値
- Key Mappingの内部キー名

`overlay_key_labels.json` は表示専用とする。

---

# 21. Overlayデザイン設定

別ファイル:

```text
%LOCALAPPDATA%\ShortcutKeyCounter\overlay-theme.json
```

初期案:

```json
{
  "version": 1,

  "position": {
    "bottom_margin": 64,
    "item_gap": 10
  },

  "key": {
    "height": 54,
    "min_width": 54,
    "padding_x": 18
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

---

# 22. 設定画面

v0.1.1では以下を追加する。

```text
オーバーレイ表示

[ON/OFF]
ショートカット入力を画面上に表示

表示位置
アクティブモニター中央下

最大表示
3件

表示時間
1.4秒

表示名マッピング
[JSONを開く]

デザイン設定
[フォルダーを開く]
```

v0.1.1では詳細アニメーション値をGUI編集可能にする必要はない。

JSON編集で対応できる構造だけ用意する。

---

# 23. 設定値

SQLite `settings` に追加:

```text
overlay_enabled = true
```

必要に応じて将来:

```text
overlay_theme = default
```

を追加する。

キーラベルとデザイン詳細はJSONファイルを使用する。

---

# 24. Overlay Eventモデル

例:

```rust
struct OverlayEvent {
    shortcut: String,
    application: String,
    source: CaptureSource,
    occurred_at: Instant,
}
```

`shortcut` は既存Normalizerで生成済みの内部表現を使用する。

通常文字入力は渡さない。

---

# 25. Overlay Itemモデル

例:

```rust
struct OverlayItem {
    id: u64,
    display_shortcut: DisplayShortcut,
    created_at: Instant,
    phase: OverlayPhase,
}
```

Phase:

```rust
enum OverlayPhase {
    Entering,
    Stable,
    Fading,
}
```

---

# 26. モジュール構成

推奨:

```text
apps/shortcut-counter/src/
└─ overlay/
   ├─ mod.rs
   ├─ manager.rs
   ├─ event.rs
   ├─ state.rs
   ├─ animation.rs
   ├─ renderer.rs
   ├─ asset_cache.rs
   ├─ shortcut_cache.rs
   ├─ config.rs
   └─ key_labels.rs
```

Windows固有:

```text
crates/shortcut-windows/src/
├─ overlay_window.rs
└─ monitor.rs
```

---

# 27. Overlay Manager責務

Overlay Managerは以下だけを担当する。

- Overlay Event受信
- Local/Elevated重複除去
- 最大3件管理
- TTL管理
- アニメーション状態管理
- モニター変更検知
- Window表示/非表示
- Renderer呼び出し

以下は担当しない。

- DB書き込み
- SQLite query
- Capture Hook
- Shortcut Normalization
- 除外アプリDB管理

---

# 28. Overlay Thread

専用Threadを1本使用する。

概念:

```text
Overlay Thread

loop
│
├─ Event待機
│
├─ Animation deadline待機
│
└─ Window message処理
```

常時ポーリングは禁止する。

待機可能なWin32メッセージ・イベント・channelを利用する。

---

# 29. Window再利用

キー入力ごとにHWNDを生成・破棄しない。

アプリ起動中にOverlay Windowを1つだけ保持する。

状態:

```text
No item
→ 非表示

Itemあり
→ 表示

全Item終了
→ 非表示
```

これによりウィンドウ生成コストを回避する。

---

# 30. DPI対応

Per-Monitor DPIを考慮する。

モニターごとに、

```text
scale = dpi / 96
```

として、

- キーサイズ
- padding
- bottom margin
- item gap
- フォントサイズ

をスケーリングする。

アクティブウィンドウが別DPIモニターへ移動した場合にも再計算する。

---

# 31. マルチモニター切替

入力ごとにForeground Windowのモニターを確認する。

直前と異なるモニターの場合:

```text
Overlay Window
↓
新Monitorの中央下へ移動
```

既存Overlay Itemが残っている場合も全体を新しいアクティブモニターへ移動する。

---

# 32. クリック透過

Overlayは完全に入力を邪魔しない。

マウス:

```text
click-through
```

キーボード:

```text
NOACTIVATE
```

とする。

Overlayをクリックして設定を開く等の操作はv0.1.1では実装しない。

---

# 33. フォーカス

Overlay表示によって、

```text
GetForegroundWindow()
```

の対象がShortcutKeyCounter自身へ変わってはならない。

表示前後でForeground Windowを奪わないことを必須条件とする。

---

# 34. エラー時

Overlayの失敗によってCaptureやDB記録を停止させてはいけない。

例:

```text
SVG読込失敗
Window生成失敗
Renderer失敗
JSON壊れ
```

の場合、

```text
Overlayのみ無効化
↓
診断ログ
↓
Capture継続
↓
DB記録継続
```

とする。

Overlayは補助機能であり、計測本体より優先してはならない。

---

# 35. JSON異常時

`overlay_key_labels.json` または `overlay-theme.json` が壊れている場合:

```text
parse error
↓
default設定へFallback
↓
ログ記録
```

ユーザーJSONを勝手に削除・上書きしない。

---

# 36. デフォルトファイル生成

初回起動時、ファイルが存在しなければデフォルトを生成する。

```text
overlay_key_labels.json
overlay-theme.json
```

既存ファイルがある場合は保持する。

---

# 37. プライバシー

Overlayへ渡してよい情報:

```text
normalized shortcut
foreground executable basename
timestamp / Instant
capture source
```

Overlayへ渡してはいけない情報:

```text
通常文字
window title
URL
document name
clipboard
full executable path
```

---

# 38. 性能方針

必須:

- Capture callbackでは`try_send`のみ
- Overlay queueはbounded
- SVGはキャッシュ
- Windowは再利用
- 全画面Window禁止
- Animation中だけtimer動作
- Item 0件でtimer停止
- 最大3件
- 合成bitmapも必要に応じてLRUキャッシュ
- Overlay失敗でCaptureを停止しない

---

# 39. 想定負荷

アイドル時:

```text
Overlay CPU ≒ 0
Overlay Window = hidden
Animation timer = stopped
```

入力直後:

```text
最大1.4秒のみ描画
最大3item
最大約600×240px
```

WebView2を追加しないため、専用WebView方式より常駐メモリ増加を抑える。

---

# 40. 受入テスト

## 40.1 基本

- Ctrl+CでOverlay表示
- Ctrl+Shift+P表示
- Alt+Tab表示
- Ctrl+MouseLeft表示
- Ctrl+WheelUp表示
- 通常文字だけでは表示されない

---

## 40.2 時間

- 160ms前後でSlide In完了
- 1.0秒時点でFade Out開始
- 1.4秒前後で完全消去

---

## 40.3 連続入力

```text
Ctrl+C
Ctrl+V
Ctrl+S
```

を連続入力。

確認:

- 最大3件
- 新規は下からSlide In
- 既存は上へ移動
- 各ItemのTTLは独立

---

## 40.4 4件目

4件連続入力。

確認:

- 最新3件だけ残る
- 最古Itemが退場
- レイアウト崩れなし

---

## 40.5 マルチモニター

- Monitor 1で入力
- Monitor 2のWindowをActive
- Monitor 2で入力

確認:

- Active Windowが存在するモニターへ表示
- DPIが異なっても適切なサイズ

---

## 40.6 除外アプリ

除外対象アプリで入力。

確認:

```text
DB記録なし
Overlay表示なし
```

---

## 40.7 Elevated

管理者権限アプリで入力。

確認:

- Overlay表示
- Local/Elevatedの二重表示なし
- Capture停止なし

---

## 40.8 フォーカス

Overlay表示中に確認:

- Active Windowが変化しない
- Keyboard focusが変化しない
- Mouse clickが下のWindowへ届く

---

## 40.9 負荷

アイドル時:

- Overlay threadがbusy loopしていない
- timerが停止している
- CPU使用増加がほぼない

連続入力:

- Hook遅延なし
- Queue overflowなし
- UI/Tauri操作へ影響なし

---

# 41. 今回実装しないもの

v0.1.1では以下を対象外とする。

- Overlayをマウス操作する機能
- Overlayクリックから設定を開く
- ユーザーによる自由配置
- Drag & Drop位置変更
- GUIで全アニメーションパラメータ編集
- 多数のテーマ同梱
- 音
- 入力履歴の永続保存
- WebViewによるOverlay
- 全画面Overlay

---

# 42. 将来拡張

将来的には以下を追加可能とする。

```text
複数Overlayテーマ
色変更
キー形状変更
Glass / Acrylic風
表示位置選択
フォント変更
サイズ変更
表示時間変更
アニメーションプリセット
マウスアイコンSVG
ゲーム配信向け大型モード
OBS向け透過モード
```

基本構造を、

```text
Renderer
Theme
Label Mapping
Animation
```

に分離しておくことで、Capture処理へ影響せず追加できる設計とする。

---

# 43. v0.1.1確定仕様

| 項目 | 採用仕様 |
|---|---|
| Overlay基盤 | Rust + Win32 Native Layered Window |
| Tauri WebView | 使用しない |
| 表示対象 | ShortcutKeyCounterが認識したショートカット |
| 除外アプリ | 表示しない |
| モニター | Foreground Windowがあるモニター |
| 位置 | 作業領域中央下 |
| 下余白 | 64px基準 |
| 最大表示 | 3件 |
| Slide/Fade In | 160ms |
| 安定表示終了 | 1.00秒 |
| Fade Out | 400ms |
| 完全消去 | 入力から1.40秒 |
| 連続入力 | 新規を下からSlide In |
| 既存項目 | 上へスライド |
| Local/Elevated重複 | Overlay専用50ms Deduper |
| DB 1秒Guard | Overlayには適用しない |
| SVG | 使用 |
| SVG処理 | 起動時/Theme変更時にキャッシュ |
| キー文字 | SVGから分離 |
| 表示名設定 | overlay_key_labels.json |
| デザイン設定 | overlay-theme.json |
| Window | 1個を再利用 |
| アイドル時 | Window非表示、timer停止 |
| Captureとの通信 | 非同期bounded queue / try_send |
| 失敗時 | Overlayのみ停止し計測本体は継続 |
| DPI | Per-Monitor DPI対応 |
| マウス | Click-through |
| Focus | NOACTIVATE |

---

# 44. 実装優先順位

実装時は以下の順番を推奨する。

```text
1. Overlay Eventモデル
↓
2. Overlay専用Queue
↓
3. Overlay Manager
↓
4. Win32 Layered Window
↓
5. Monitor / DPI位置決定
↓
6. 単一Item表示
↓
7. SVG Renderer / Cache
↓
8. 160ms Slide In
↓
9. 400ms Fade Out
↓
10. 最大3件Stack
↓
11. Local/Elevated 50ms Deduper
↓
12. overlay_key_labels.json
↓
13. overlay-theme.json
↓
14. Settings ON/OFF
↓
15. 性能試験
↓
16. Elevated / Multi-monitor / DPI受入
```

---

# 45. 完了条件

v0.1.1のOverlay機能は、以下をすべて満たした場合に完成とする。

- Shortcut入力で即時表示される
- Active Windowのあるモニターへ表示される
- 中央下に正しく配置される
- 最大3件表示される
- 新規Itemが滑らかにSlide Inする
- 1秒経過後Fade Outする
- 1.4秒で完全に消える
- 通常文字入力は表示されない
- 除外アプリでは表示されない
- Local/Elevatedによる二重表示がない
- OverlayがActive Windowを奪わない
- OverlayがMouse inputを奪わない
- アイドル時timerが停止する
- SVGを入力ごとにparseしない
- キー表示名をJSONで変更できる
- デザインをSVG/JSON差し替えで変更できる
- Overlay障害が計測・DB記録へ波及しない
- マルチモニター/DPI環境で動作する
- 長時間常駐時にCPU/メモリが増加し続けない

以上を ShortcutKeyCounter v0.1.1 のオーバーレイ表示機能の基本設計とする。
