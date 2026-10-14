# ビルド・検証コマンド

すべてプロジェクトルート `C:\develop\ShortcutKeyCounter` で `pwsh` から実行します。

## Release ビルド

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

## 既存 exe の WebView lifecycle 試験

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-exe-ShortcutKeyCounter-webview-lifecycle.ps1
```

`target/release/ShortcutKeyCounter.exe` が存在しない場合は失敗し、暗黙のビルドへ切り替えません。GUI を起動してプロセスを操作するため、対話可能な Windows セッションで実行します。

## ZIP 生成

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\package-ShortcutKeyCounter-zip.ps1 -Version 0.1.4
```

既存の Release exe から `release/Publish/` にZIPを生成します。ビルドや外部公開は行いません。
