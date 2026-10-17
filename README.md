# ShortcutKeyCounter

> Windowsで使ったショートカットキーを、端末内だけで記録・可視化するデスクトップアプリです。

対応OS: Windows 10 / 11（64bit）｜現在のリリース: v0.1.4｜ライセンス: MIT

<!--
README用の画像を追加したら、次のコメントを画像参照へ置き換えてください。
![ShortcutKeyCounterのダッシュボード](docs/images/users-guide/006-dashboard-overview.png)
-->

## 概要

ShortcutKeyCounterはWindowsのバックグラウンドに常駐し、入力されたショートカットキーを記録・集計するアプリです。キー別、アプリ別、時間軸で利用傾向を可視化し、「どのアプリで、どのショートカットが実際に使われているか」を確認できます。

Keyvizのように、入力したキーを画面上へポップアップ表示するショートカットキービジュアライズ機能も搭載しています。作業配信や画面共有で使えるよう、表示するキー、位置、外観、アニメーションを細かく設定できます。

計測データはローカルのSQLiteへ保存します。入力した文章、URL、ウィンドウタイトル、ドキュメント名、クリップボード、実行ファイルのフルパスは保存しません。

## 主な機能

- ショートカットの回数をキー別・アプリ別に集計
- 今日・7日・30日・全期間のランキングと使用率グラフ
- リアルタイムログ、キー別ランキング、アプリ別キーランキング
- キーとアプリの組み合わせを確認できるヒートマップ形式のマトリクス表示
- 入力したキーを画面へ表示するネイティブオーバーレイ
- キー・修飾パターンごとの表示／記録ポリシー設定
- 計測しないアプリの除外設定
- CSV出力、SQLiteバックアップ、Excel等での追加分析
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

## ショートカットキービジュアライズ

作業配信でも使いやすいよう、オーバーレイは次の項目をカスタマイズできます。

- 画面に表示するキーと修飾キーパターンの全組み合わせ
- 日本語キーボード固有キーを含む表示名
- デスクトップ／アクティブウィンドウ、モニター、9か所の表示位置
- 背景色、透明度、表示時間、最大行数、IN/OUTアニメーション
- 同じキーを3回以上連続入力した場合の `×N` 圧縮表示
- 連打カウンターの色、形状、角度、フォントサイズ

連打を `×N` にまとめて画面を埋めにくくする仕組みは、特にこだわった機能です。より楽しいアニメーションやデザインのアイデアは[GitHub Issues](https://github.com/KotorinChunChun/ShortcutKeyCounter/issues)で募集しています。

## ダウンロード

開発版の本体連携は[KotorinToysからの管理](docs/kotorintoys-integration.md)を参照してください。公開済みZIPの管理窓口対応を意味するものではありません。

現在は暫定版をGitHubで公開しています。

1. [GitHub Releases](https://github.com/KotorinChunChun/ShortcutKeyCounter/releases)から最新の `ShortcutKeyCounter-vX.Y.Z-win-x64.zip` をダウンロードします。
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

- 入力した文章や、修飾キーなしの英字・数字・記号・テンキー入力
- URL、ウィンドウタイトル、ドキュメント名
- クリップボード内容
- 実行ファイルのフルパス
- パスワードや資格情報

Security Gateは、オーバーレイ表示、リアルタイムログ、DB記録より前に動作します。パスワード・資格情報入力、高権限アプリ、除外アプリ、安全性を確認できない状態では、記録しない側へ停止します。

修飾キーなしの英字・数字・記号・テンキー入力は、UIやJSON設定を変更しても永続記録できません。Fキー・システムキー・マウス・ホイールは、キーマッピングで明示的に選択し、Security Gateを通過した場合に限り記録できます。

計測・集計・CSV機能はネットワーク通信を行わず、計測データを外部へ自動送信しません。ヘルプ画面からGitHubを開く操作を行った場合だけ、既定ブラウザーが外部サイトへアクセスします。

### 利用上の注意

キー入力の監視にはセキュリティ上のリスクがあります。本ツールは安全側に停止する対策を実装していますが、あらゆるアプリや入力欄を完全に判定できることは保証できません。利用環境を確認したうえで、十分に注意して使用してください。

特に、単独キーのオーバーレイ表示を有効にしたまま画面共有や配信を行うと、想定外の入力欄がパスワード欄として検出されなかった場合に、入力内容が画面へ表示される可能性があります。パスワードや機密情報を入力する前に、画面共有を停止するか、オーバーレイと計測を停止してください。

## CSV分析と統計データ共有

日次集計と詳細ショートカット履歴はCSVへ出力できるため、Excel等で独自に分析したり、内容を確認したうえで他の人へ共有できます。

もしよければ、みんなで本当に有用なショートカットキーランキングを作るため、匿名化した集計データの共有にご協力ください。共有方法は[GitHub Issues](https://github.com/KotorinChunChun/ShortcutKeyCounter/issues)でご相談ください。

共有は完全に任意です。CSVにはショートカット名、アプリ名、利用時刻等が含まれる場合があります。共有前に必ず内容を確認し、個人・勤務先・作業内容を推測できる情報を削除してください。SQLite DBや未確認の詳細ショートカット履歴CSVを、そのまま公開しないでください。本ツールが統計データを自動送信することはありません。

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
- `dev/scripts`: ビルド、テスト、ローカルパッケージ処理
- `docs`: 利用者・ビルド担当者向け文書
- `dev`: 現行仕様、実装記録、配布手順、旧版履歴
- `release/Publish`: 配布成果物のローカル控え（Git追跡対象外）

### 必要な環境

- Rust stable
- Node.js
- PowerShell 7（`pwsh`）
- Windows SDKを含むRustのWindowsビルド環境

### ビルド

Releaseビルドだけを行う場合:

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\build-release-ShortcutKeyCounter-all.ps1
```

自動品質検証をまとめて行う場合:

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-release-ShortcutKeyCounter-all.ps1
```

ZIP生成はビルドと分離されており、既存のRelease成果物に対して `package-ShortcutKeyCounter-zip.ps1` を実行します。

個別に確認する場合:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
pwsh -ExecutionPolicy Bypass -File .\apps\shortcut-key-counter\scripts\build-ui.ps1
```

詳しくは[開発者ガイド](docs/DEVELOPERS_GUIDE.md)と[ビルド・検証コマンド](docs/BUILD_COMMANDS.md)を参照してください。

## ドキュメント

- [ユーザーガイド](docs/users-guide.md)
- [利用者向けドキュメント一覧](docs/README.md)
- [開発者ガイド](docs/DEVELOPERS_GUIDE.md)
- [ビルド・検証コマンド](docs/BUILD_COMMANDS.md)
- [Windowsビルド・受入手順](docs/windows-build-and-acceptance.md)
- [開発資料](dev/README.md)

## ライセンス

本ソフトウェアは[MIT License](LICENSE.txt)で公開しています。

依存ライブラリには各ライセンスが適用されます。[第三者ライセンスと画像素材の出典](docs/third-party-licenses.md)を参照してください。配布ZIPには`third-party/`の表示文とMPL対象の原本ソースを同梱します。

## 作者・フィードバック

- GitHub: [KotorinChunChun](https://github.com/KotorinChunChun)
- 不具合報告・要望: [GitHub Issues](https://github.com/KotorinChunChun/ShortcutKeyCounter/issues)
