# ShortcutKeyCounter v0.1.4

Windows上のショートカット利用をローカル集計する Rust + Tauri 2 デスクトップアプリです。通常文字列、URL、ウィンドウタイトル、ドキュメント名、クリップボード、実行ファイルのフルパスは保存しません。

## v0.1.4 主な実装

- Win32 Low-Level keyboard / mouse hook + Elevated helper。
- 修飾なし単独入力はkeyboard / mouse / vertical wheel / horizontal wheelのすべてで永続記録禁止。修飾なし通常mouse clickはOverlay表示のみ可能、wheel-only / Modifier-onlyはOverlayにも表示しない。
- Realtime LogはDB記録対象イベントだけを表示し、Security GateはOverlay / Realtime Log / DBより前でFail Closed。
- Overlay設定のcombo再描画問題と固定monitor選択を修正。Light themeの9位置選択状態も青背景・白文字へ統一。
- DuplicateGuardを0.0〜10.0秒、詳細イベント保持を1〜365日でruntime設定化。
- キーマッピングは内部scroll、分類filter + 一括変更、修飾列multi-select、検索専用の物理キーCaptureを実装。検索入力はDB/Realtime Log/Overlay/statisticsへ流さない。
- 「計測結果」配下へRealtime Log / Ranking / Usage Graph / Key×Application Matrixを整理。Rankingはキー別とアプリ別を左右2ペインへ統合。
- Rankingは今日 / 7日 / 30日 / 全期間、対象アプリ再集計、平均・構成比・比較率、3-state sort（降順→昇順→なし）へ対応。今日の比較は前日の同時刻まで。
- Usage Graphを追加。キー/アプリ別、上位5系列初期ON、今日=hour・7/30日=day・全期間=monthでSQLite集計し、系列色を内部identity単位で保存。
- Native Overlayに700ms Repeat Compressorを追加。2回目までは通常表示、3回目から `×N` へ圧縮し、countだけ専用bump animation。DB DuplicateGuardとは独立。
- Repeat counterの背景/文字/影色、badge形状、回転角、font sizeを設定画面内PreviewとNative Overlayで共通schema化。`overlay-theme.json`はschema v3。
- Overlay退避clickはclick-throughを維持しつつ、修飾付きでもShortcutKeyCounter内部では完全破棄。中央からの退避先は右下。
- Production / Test DBを維持。Sample dataはTest DBだけへ、Production DBの記録可能identityを母集団として前日まで60日・約10,000event + application usageを生成。前日Realtime Log sampleはTest DBの生成済みeventから構成。
- v0.1.3ユーザー修正版のCargo dependency / Overlay `Arc<Pixmap>` / Clippy / test修正を維持。

## セキュリティ不変条件

設定UI・JSON・旧設定値よりRuntime Security Gateを優先します。特に修飾なし単独入力は設定を改ざんしてもSQLite、Realtime Log、診断、CSV等へ保存しません。password/credential保護、高権限foreground、計測停止、除外アプリの判定はOverlay/DBより前に適用します。

## プロジェクト構成

- `crates/shortcut-common`: 共通model / IPC protocol / path
- `crates/shortcut-core`: shortcut判定・正規化・重複抑制
- `crates/shortcut-storage`: SQLite、repository、statistics、migration
- `crates/shortcut-windows`: Win32 hook / foreground / user activity / overlay window / elevation / IPC
- `apps/shortcut-key-counter`: Tauriメインアプリ
- `apps/shortcut-key-capture`: 管理者権限アプリ向け入力取得helper
- `scripts`: release / ZIP packaging
- `docs`: Windows build / acceptance
- `dev`: 仕様・検証資料

## Windows build

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\scripts\build-release.ps1
```

ZIPだけ作る場合:

```powershell
.\scripts\build-release.ps1 -SkipInstaller
```

個別確認:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release -p shortcut-key-counter -p shortcut-key-capture
```

Frontend:

```powershell
tsc -p .\apps\shortcut-key-counter\ui\tsconfig.json
```

詳細は `docs/WINDOWS_BUILD_AND_ACCEPTANCE.md` を参照してください。

## データ保存先

```text
%LOCALAPPDATA%\ShortcutKeyCounter\ShortcutKeyCounter.db
%LOCALAPPDATA%\ShortcutKeyCounter\ShortcutKeyCounter.test.db
%LOCALAPPDATA%\ShortcutKeyCounter\ShortcutKeyCounter_key_mapping.json
%LOCALAPPDATA%\ShortcutKeyCounter\overlay_key_labels.json
%LOCALAPPDATA%\ShortcutKeyCounter\overlay_key_policy.json
%LOCALAPPDATA%\ShortcutKeyCounter\overlay-theme.json
%LOCALAPPDATA%\ShortcutKeyCounter\overlay\default\key.svg
```

DBは実行ファイル配置先から独立しています。
