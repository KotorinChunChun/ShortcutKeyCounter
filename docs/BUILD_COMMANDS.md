# ビルド・検証コマンド

すべてプロジェクトルート `C:\develop\ShortcutKeyCounter` で `pwsh` から実行します。

## Release ビルド

本体連携を含むDebugビルドのみを行うには、`pwsh -ExecutionPolicy Bypass -File .\dev\scripts\build-debug-ShortcutKeyCounter-all.ps1`を使います。実行ファイルは`target/debug/`へ生成し、起動・導入・外部公開は行いません。

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\build-release-ShortcutKeyCounter-all.ps1
```

この入口は UI と2つの実行ファイルを Release 構成でビルドします。テスト、ZIP生成、インストーラ生成、外部公開は行いません。

## 自動品質検証

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-release-ShortcutKeyCounter-all.ps1
```

静的回帰ゲート、format、check、clippy、workspace test、Release ビルドを実行し、いずれかが失敗すると非0で終了します。既存 Release exe に対する WebView lifecycle 試験も続ける場合は `-RunWebViewLifecycleTest` を指定します。

## 個別コマンド

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pwsh -ExecutionPolicy Bypass -File .\apps\shortcut-key-counter\scripts\build-ui.ps1
```

## 既存 Debug exe の画面を出さない UI 試験

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-exe-ShortcutKeyCounter-ui-background.ps1
```

既存の `target/debug/ShortcutKeyCounter.exe` を隔離試験モードで起動し、ウィンドウを表示しないまま UI を CLI から自動判定します。exe がない場合は失敗し、暗黙のビルドへ切り替えません。先に `build-debug-ShortcutKeyCounter-all.ps1` を実行してください。Node.js 22.13 以上が必要です。

- 対象: UI と同じ経路での Tauri command 呼び出し、統計・期間の検証、CSV・バックアップ・JSON 出力、設定・除外アプリ、キーマッピングの未保存ガード、DB 切替とサンプル生成、全 11 画面の表示と JS 例外の有無。各画面のスクリーンショットと `report.json` を出力先に保存します。
- 隔離: 保存先・出力先・WebView2 profile は出力先フォルダー内の `localappdata` に置きます。入力 Hook、自動起動の同期、Overlay、本体管理窓口は使わず、単一起動も通常版と別の名前空間で判定するため、通常版を起動したままでも実行できます。
- 判定: 試験項目に加えて、利用者に見えるウィンドウを表示していないこと、前面ウィンドウを取得していないこと、通常版の保存先を変更していないことを確認します。
- 対象外: Overlay の実描画、実際のキー・マウス入力の Hook、トレイ操作、UAC、フォルダー・URL を開く操作は、前面表示や実入力が必要なため Windows 実機受入で確認します。
- 出力先は既定で一時フォルダーです。`-OutputDirectory` で変更できます。

隔離試験モードは Debug ビルドだけで有効です。Release exe を指定した場合は、本番の保存先と入力 Hook を使うため実行を拒否します。手動で起動する場合は、次の環境変数をそのプロセスだけに設定します。

| 環境変数 | 内容 |
|---|---|
| `SHORTCUT_KEY_COUNTER_TEST_LOCALAPPDATA` | 隔離先の絶対パス。保存先・出力先・WebView2 profile を隔離し、Hook・自動起動同期・Overlay・管理窓口を止める |
| `SHORTCUT_KEY_COUNTER_TEST_BACKGROUND_UI` | `1` のとき、UI を非表示・非アクティブ・タスクバー非表示で生成する。上の変数がない場合は無視する |

## 既存 exe の WebView lifecycle 試験

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-exe-ShortcutKeyCounter-webview-lifecycle.ps1
```

`target/release/ShortcutKeyCounter.exe` が存在しない場合は失敗し、暗黙のビルドへ切り替えません。GUI を起動してプロセスを操作するため、対話可能な Windows セッションで実行します。

## ZIP 生成

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\package-ShortcutKeyCounter-zip.ps1 -Version 0.1.5
```

既存のRelease exeから`dev/history/vX.Y.Z/logs/package/Publish/`にZIPを生成します。実行ごとの出力先は対象版のlogs配下へ`-OutputDir`と`-StageRoot`で指定できます。third-partyの表示文・MPL原本も同梱し、古い台帳・原本改変・必須表示欠落を拒否します。ビルドと外部公開は別工程です。
