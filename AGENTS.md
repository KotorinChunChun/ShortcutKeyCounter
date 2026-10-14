# ShortcutKeyCounter 開発ルール

更新日: 2026-09-06

本書を、プロジェクト固有の必須ルール、配置、資料の参照条件の正本とする。本文・コミットログは日本語で記述し、PowerShell は Windows PowerShell ではなく `pwsh` を使用する。既存の利用者向け挙動とセキュリティ規則を、一般テンプレートより優先する。

## 作業の基本

- 最初に `git status --short` を確認し、既存のユーザー変更を保持する。
- 概要は `README.md`、詳細は下の目的別目録から現在の作業に必要な資料だけを読む。
- 要求一覧は `dev/vX.Y.Z-req.md`、AI の実装計画・報告は `dev/vX.Y.Z-imp.md`、要求が粗い場合の進捗は `dev/vX.Y.Z-task.md` に記録する。
- 実装予定・実装中の版別資料は `dev/` 直下、完成した一式は `dev/history/vX.Y.Z/` に置く。版番号や次版開始だけで完成扱いにしない。
- 未完了事項を別版または roadmap へ移す場合は、元資料と引き継ぎ先の双方に出典と参照を残す。
- 過去の未チェック欄だけで未実装と判断せず、後続の完了記録とユーザー確認を照合する。工程終了の確認と、新しい測定証跡の取得は区別する。
- 多数の実装はフェーズとチェック項目に分け、各フェーズでコンパイルまたはテストに成功してから日本語の Conventional Commit 形式でコミットする。
- ユーザーに実機テストを依頼するときは、プロジェクトへ移動する `cd` とデバッグ起動コマンドを提示する。

## 配置

| 配置 | 役割 |
|---|---|
| `README.md` | 利用者の入口と提供物の概要 |
| `docs/` | 利用者、および自分でビルド・改造する第三者向けの公開資料 |
| `dev/` | 開発者本人の計画、判断、作業記録 |
| `dev/scripts/` | ビルド、起動、検証、ローカルパッケージ生成、公開の入口 |
| `dev/private/` | 秘密情報・個人情報を含む非公開資料。Git・公開成果物・通常の調査対象外 |
| `dev/temp/` | プロジェクト外の一時制作物。Git・目録・通常の調査対象外 |
| `apps/` | 実行アプリごとのソースと固有リソース |
| `crates/` | Rust workspace の共通ライブラリ |
| `migrations/` | 製品同梱の SQLite migration |
| `tests/` | 複数構成で共有する試験、fixtures、期待結果 |
| `release/` | 配布成果物。Git 対象外 |

一般テンプレートの `src/` に相当する実装は、本プロジェクトでは責務別の `apps/` と `crates/` に分ける。`apps/shortcut-key-counter/scripts/build-ui.ps1` は Tauri の `beforeDevCommand` / `beforeBuildCommand` から呼ばれるアプリ内部フックであるため、`dev/scripts/` へ移さない。

不要な空フォルダーや取り込み用 `templates/` は保存しない。正式な開発資料、開発経緯、運用スクリプト、テストは追跡する。秘密鍵・認証情報の実値は公開メタデータと区別し、必要なら `dev/private/` へ置く。プロジェクト外の一時物は `dev/temp/` に置き、ユーザーが明示した場合だけ読む。

## 目的別の資料目録

| 作業・目的 | 読む資料 |
|---|---|
| 概要・利用方法 | `README.md`、`docs/users-guide.md` |
| ビルド・改造 | `docs/DEVELOPERS_GUIDE.md`、`docs/BUILD_COMMANDS.md` |
| Windows 実機受入 | `docs/windows-build-and-acceptance.md` |
| 要求の実装 | 対象版の `dev/vX.Y.Z-req.md` と対応する `imp`・`task` |
| 文書配置・命名 | `dev/documentation-rules.md` |
| 将来機能 | `dev/PLAN_IDEAS.md` と必要な `dev/vX.Y.Z-design-*.md` |
| 配布・公開 | `dev/RELEASE_RULE.md` |
| 過去の判断・結果 | `dev/history/` の必要な版と後続の完了記録 |

## 製品固有のセキュリティ・データ規則

設定、互換値、UI 要求より次の規則を優先する。

1. `RULE-SEC-001` / `RULE-SEC-006`: 修飾なしの英字、数字、記号、テンキー数字・演算子は、Overlay 用の一時メモリ処理を除き永続化しない。UI、JSON、DB の改ざんでも解除できない。修飾キー単独は記録しない。F キー、システムキー、マウス、ホイールは、明示的な KeyPolicy と Security Gate の通過時だけ記録できる。
2. `RULE-SEC-002`: Realtime Log へ送るのは最終的な DB 記録対象だけとする。DisplayOnly、Ignore、除外アプリ、保護中、計測停止中の入力を送らない。
3. `RULE-SEC-003` / `RULE-SEC-004`: password・資格情報・secure desktop・判定不能状態では Fail Closed とし、Overlay、DB、Realtime Log、統計、入力を含む診断を止める。Security Gate は表示・記録・DB writer より前に通す。
4. `RULE-SEC-007`: 通常権限プロセスから、より高い Integrity Level の foreground 入力を表示・記録しない。権限判定失敗時も Fail Closed とし、UIAccess を既定設計にしない。
5. 保存対象は正規化済みショートカット、foreground executable basename、集計時刻、機能有効時の日次利用秒数に限定する。通常文字列、タイトル、URL、文書名、password、入力欄の文字列は保存しない。
6. Hook callback は bounded queue へ軽量イベントを渡すだけとし、SQLite、ファイル I/O、UI 更新、重い UI Automation、ネットワーク、長時間 lock、大量文字列処理を行わない。
7. `RULE-SEC-008`: Overlay の `S A M P L E` preview は実データ経路、DB、Realtime Log、CSV、診断、統計へ流さない。
8. `RULE-DATA-013`〜`015`: サンプル生成はユーザーの明示操作時だけ検証 DB へ行う。本番 DB を母集団として読めても書き込まない。通常 Capture へ偽入力を注入せず、記録禁止入力を生成せず、Realtime Log の通常仕様を弱めない。
9. `RULE-SEC-010`: `WheelLeft` / `WheelRight` は Security Gate 後に KeyPolicy で記録可否を決める。修飾なしは明示設定時だけ記録し、単独ホイールは Overlay へ表示しない。
10. `RULE-UI-011`: Ctrl、Alt、Shift、Win、各 Wheel の単独入力は Overlay 表示しない。通常キー、文字、数字、F キー、MouseLeft / Right / Middle / X1 / X2 の単独入力は表示可能とし、修飾付き Wheel は KeyPolicy に従う。
11. `RULE-UI-012` / `013` / `018`: Overlay の click-through を維持する。退避クリックは global mouse 座標と Overlay bounds で検出し、修飾付きでも ShortcutKeyCounter 内では DB、Realtime Log、Overlay 入力、日次集計、統計から完全破棄する。背面アプリへの元クリックは通す。
12. `RULE-SEC-016`: キーマッピング検索が armed 中の物理入力は Security Gate 後に検索処理で完全消費し、表示・記録・統計へ流さない。検索結果は normalized key identity を使う。
13. `RULE-UI-017`: Overlay Repeat Compressor と DB DuplicateGuard を分離する。同一 identity の 700 ms 以内を一つの run とし、1回目は通常、2回目は2入力、3回目以降は `×N` 表示とする。更新時に既存 keycap の IN/OUT clock を再始動しない。
14. `RULE-DATA-019`: 使用率グラフは SQLite で今日=時間、7日/30日=日、全期間=月へ集計し、詳細イベント全件を frontend へ返さない。系列色は内部 identity 単位で保存する。

各リリースで、記録禁止入力、password 欄、除外アプリ、Local/Elevated 重複抑制、Overlay の click-through と非 focus、Capture 停止中の Tray 状態を最低限回帰確認する。

## スクリプトと配布

製品操作用スクリプトは `動詞-モード-ShortcutKeyCounter-機能名[-検証データ].ps1` とし、`build` はビルドだけ、`run` はビルド後の起動、`test` は固定条件の自動判定、`publish` は配布物生成と外部公開を完遂する入口に限定する。既存成果物を使うモードは `exe` とし、成果物がなければ失敗させる。

ローカルパッケージ生成は補助処理として `package-ShortcutKeyCounter-<形式>.ps1` を使う。公開を伴わない処理を `publish` と呼ばない。公開先、形式、署名、コマンドは `dev/RELEASE_RULE.md` を正とし、ユーザーの明示依頼なしに外部公開・署名を行わない。

## Git とコミット

`dev/private/`、`dev/temp/`、`release/`、秘密鍵付き証明書を Git 対象外にする。既に秘密情報がコミットされていた場合は値を転載せず、失効・再発行と履歴除去の必要性を報告する。

コミットは `<type>: <日本語の要約>` とし、`feat`、`fix`、`docs`、`style`、`refactor`、`chore` を使う。原則として1機能または1修正を、ビルド・テストが通る単位で記録する。既存の未コミット変更を無断で同梱しない。
