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
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\package-ShortcutKeyCounter-zip.ps1 -Version 0.1.5
```

入力は `target/release/ShortcutKeyCounter.exe` と `ShortcutKeyCapture.exe`、既定出力は `dev/history/vX.Y.Z/logs/package/Publish/ShortcutKeyCounter-vX.Y.Z-win-x64.zip` である。実行ごとの`-OutputDir`と`-StageRoot`も対象版のlogs配下へ指定する。ZIPにDB・設定・秘密鍵を含めず、third-partyの表示文とMPL原本を同梱する。Cargo.lockの不一致・原本改変・必須表示欠落は拒否する。

## 3. 受入と外部公開

ZIPの展開内容、SHA-256、`docs/windows-build-and-acceptance.md`の項目を確認する。ユーザーがリリースを指示した場合は、検証したソースをコミット・pushし、そのコミットに新しいタグを付け、GitHub ReleasesへZIPとSHA256SUMS.txtを公開する。既存タグ・assetを上書きしない。未確認の実機受入項目は版別報告とRelease本文に記載する。

インストーラの定義ファイルは現在のリポジトリに存在しないため、インストーラ生成は現行の配布工程に含めない。
