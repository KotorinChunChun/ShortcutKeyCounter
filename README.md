# ShortcutKeyCounter

> Windowsで使ったショートカットキーを、端末内だけで記録・可視化するデスクトップアプリです。

対応OS: Windows 10 / 11（64bit）｜現在のリリース: v0.1.4｜ライセンス: MIT

<!--
README用の画像を追加したら、次のコメントを画像参照へ置き換えてください。
![ShortcutKeyCounterのダッシュボード](docs/images/users-guide/006-dashboard-overview.png)
-->

## 概要

ShortcutKeyCounterは、普段どのショートカットをどのアプリで使っているかを集計するWindowsアプリです。ランキングやグラフで利用傾向を確認でき、よく使う操作や覚えたいショートカットの振り返りに利用できます。

計測データはローカルのSQLiteへ保存します。入力した文章、URL、ウィンドウタイトル、ドキュメント名、クリップボード、実行ファイルのフルパスは保存しません。

## 主な機能

- ショートカットの回数をキー別・アプリ別に集計
- 今日・7日・30日・全期間のランキングと使用率グラフ
- キーとアプリの組み合わせを確認できるマトリクス表示
- 入力したキーを画面へ表示するネイティブオーバーレイ
- キー・修飾パターンごとの表示／記録ポリシー設定
- 計測しないアプリの除外設定
- リアルタイムログ、CSV出力、SQLiteバックアップ
- 本番DBと検証DBの分離、検証用サンプルデータ生成

## 画面構成

| 画面 | できること |
|---|---|
| ダッシュボード | 利用回数、ランキング、利用推移の概要を確認 |
| リアルタイムログ | 記録対象になったショートカットをメモリ上で確認 |
| ランキング | キー別・アプリ別の集計を期間ごとに比較 |
| 使用率グラフ | 上位キー・アプリの推移をグラフ表示 |
| キー × アプリ見取り図 | キーとアプリの利用傾向をマトリクス表示 |
| キーマッピング | キー名と表示／記録ポリシーを編集 |
| 除外アプリ | 計測対象外にする実行ファイル名を管理 |
| データ管理 | DB切替、CSV、バックアップ、削除を実行 |
| 設定 | オーバーレイ、Windows起動、保持期間等を変更 |

## ダウンロード

1. [GitHub Releases](https://github.com/KotorinChunChun/ShortcutKeyCounter/releases/latest)から最新の `ShortcutKeyCounter-vX.Y.Z-win-x64.zip` をダウンロードします。
2. ZIPを任意のフォルダーへ展開します。
3. `ShortcutKeyCounter.exe` と `ShortcutKeyCapture.exe` を同じフォルダーに置いたまま、`ShortcutKeyCounter.exe` を実行します。

インストーラを使わないポータブル版です。実行ファイルはコード署名されていないため、Windowsの警告が表示される場合があります。実行前に、公式GitHubから取得したファイルであることと、Release本文のSHA-256が一致することを確認してください。

詳しい導入方法は[ユーザーガイド](docs/users-guide.md)を参照してください。

## 基本的な使い方

1. `ShortcutKeyCounter.exe` を起動します。起動と同時に計測が始まります。
2. 普段どおりWindowsアプリを操作します。
3. ダッシュボード、ランキング、使用率グラフで結果を確認します。
4. 必要に応じてキーマッピング、除外アプリ、オーバーレイを調整します。

ウィンドウを閉じてもバックエンドは通知領域で動作を続けます。画面の再表示は通知領域アイコンのダブルクリック、完全終了は右クリックメニューの「終了」から行います。

## プライバシーとセキュリティ

保存する情報:

- 正規化済みのショートカット名
- 最前面アプリの実行ファイル名
- 集計に必要な時刻と回数
- 有効にした場合のアプリ利用時間の日次集計

保存しない情報:

- 入力した文章や修飾キーなしの単独入力
- URL、ウィンドウタイトル、ドキュメント名
- クリップボード内容
- 実行ファイルのフルパス
- パスワードや資格情報

Security Gateは、オーバーレイ表示、リアルタイムログ、DB記録より前に動作します。パスワード・資格情報入力、高権限アプリ、除外アプリ、安全性を確認できない状態では、記録しない側へ停止します。

修飾キーなしの単独入力は、UIやJSON設定を変更しても永続記録できません。

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

データは実行ファイルの配置先とは別に保存されます。アンインストール時に統計や設定も削除する場合は、先にバックアップを作成してください。

## 開発者向け情報

### 構成

- `apps/shortcut-key-counter`: Tauriメインアプリ
- `apps/shortcut-key-capture`: 管理者権限アプリ向け入力取得ヘルパー
- `crates/shortcut-common`: 共通モデル、IPC、パス
- `crates/shortcut-core`: 判定、正規化、重複抑制
- `crates/shortcut-storage`: SQLite、Repository、統計、Migration
- `crates/shortcut-windows`: Win32 Hook、Foreground、Overlay、Elevation
- `migrations`: SQLite Migration
- `scripts`: ビルド、テスト、パッケージ処理
- `docs`: 利用者・ビルド担当者向け文書
- `dev`: 現行仕様、実装記録、テンプレート、旧版履歴
- `release/Publish`: 配布成果物のローカル控え（Git追跡対象外）

### 必要な環境

- Rust stable
- Node.js
- PowerShell 7（`pwsh`）
- Windows SDKを含むRustのWindowsビルド環境

### ビルド

ZIPまで生成する場合:

```powershell
pwsh -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -SkipInstaller
```

生成したZIPは `release/Publish/` に保存されます。

個別に確認する場合:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pwsh -ExecutionPolicy Bypass -File .\apps\shortcut-key-counter\scripts\build-ui.ps1
```

詳しくは[Windowsビルド・受入手順](docs/windows-build-and-acceptance.md)を参照してください。

## ドキュメント

- [ユーザーガイド](docs/users-guide.md)
- [利用者向けドキュメント一覧](docs/README.md)
- [Windowsビルド・受入手順](docs/windows-build-and-acceptance.md)
- [開発資料](dev/README.md)

## ライセンス

本ソフトウェアは[MIT License](LICENSE.txt)で公開しています。

## 作者・フィードバック

- GitHub: [KotorinChunChun](https://github.com/KotorinChunChun)
- 不具合報告・要望: [GitHub Issues](https://github.com/KotorinChunChun/ShortcutKeyCounter/issues)
