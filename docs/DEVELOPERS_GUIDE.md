# 開発者ガイド

ShortcutKeyCounter は Windows 向けの Rust workspace で、Tauri メインアプリ、入力取得ヘルパー、責務別の共通 crate から構成されます。

## 開発環境

- Windows 10 / 11 x64
- Rust stable（MSVC target）
- Visual Studio Build Tools の「C++ によるデスクトップ開発」
- Node.js
- Microsoft Edge WebView2 Runtime
- PowerShell 7（`pwsh`）

## 構成

- `apps/shortcut-key-counter`: Tauri メインアプリと TypeScript UI
- `apps/shortcut-key-capture`: 高権限アプリ向け入力取得ヘルパー
- `crates/shortcut-*`: 共通モデル、判定、保存、Windows 統合
- `migrations/`: SQLite migration
- `dev/scripts/`: ビルド、品質検証、既存 exe の試験、ZIP 生成

開発時も `AGENTS.md` の Security Gate、データ最小化、権限境界を変更しないでください。UI の変更時はアプリ内部フック `apps/shortcut-key-counter/scripts/build-ui.ps1` で TypeScript をコンパイルします。

実行可能なコマンドは [BUILD_COMMANDS.md](./BUILD_COMMANDS.md)、Windows の受入項目は [windows-build-and-acceptance.md](./windows-build-and-acceptance.md) を参照してください。配布担当者向けの手順は `dev/RELEASE_RULE.md` に分離しています。
