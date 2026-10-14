# ShortcutKeyCounter 配布手順

更新日: 2026-09-06

本書を本プロジェクトの配布手順の正本とする。現在の配布形式は Windows x64 ポータブル ZIP、公開先は GitHub Releases である。コード署名と自動公開スクリプトは現時点で採用していない。

## 前提と禁止事項

- Rust stable、Node.js、PowerShell 7（`pwsh`）を使用する。
- バージョンは workspace の `Cargo.toml`、Tauri 設定、UI 表示、配布文書で一致させる。
- ビルド、ローカルパッケージ生成、外部公開を別工程として扱う。
- 通常のビルド・検証では GitHub Release の作成、アップロード、署名を行わない。
- 外部公開と署名は、ユーザーがその工程を明示的に依頼した場合だけ実施する。

## 1. 品質検証と Release ビルド

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-release-ShortcutKeyCounter-all.ps1
```

静的回帰ゲート、Rust の format/check/clippy/test、UI と Release exe のビルドがすべて成功することを確認する。GUI を使う WebView lifecycle 試験は必要な受入環境で `-RunWebViewLifecycleTest` を付けて実行する。

ビルドだけが必要な場合は次を使う。

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\build-release-ShortcutKeyCounter-all.ps1
```

## 2. ZIP のローカル生成

`Cargo.toml` の版と同じ値を明示する。

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\package-ShortcutKeyCounter-zip.ps1 -Version 0.1.4
```

入力は `target/release/ShortcutKeyCounter.exe` と `ShortcutKeyCapture.exe`、出力は `release/Publish/ShortcutKeyCounter-vX.Y.Z-win-x64.zip` である。ZIP に DB、設定、秘密鍵を含めない。

## 3. 受入と外部公開

ZIP の展開内容、SHA-256、`docs/windows-build-and-acceptance.md` の必須項目を確認する。GitHub Releases へのタグ作成、Release 本文作成、ZIP と SHA-256 のアップロードは手動工程であり、本ルール適用時には実行しない。

インストーラの定義ファイルは現在のリポジトリに存在しないため、インストーラ生成は現行の配布工程に含めない。
