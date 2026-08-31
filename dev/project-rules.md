# ShortcutKeyCounter PROJECT_RULES

更新日: 2026-09-01

## 1. 最優先セキュリティ原則

### RULE-SEC-001 修飾なし英数記号入力を永続化しない

修飾キーなしの英字、数字、記号、テンキー数字・演算子は、オーバーレイ表示のための一時的なメモリ処理以外では保持してはならない。

ファンクションキー、システムキー、マウスクリック、垂直・水平ホイールは、KeyPolicyで明示的に記録対象とし、Security Gateを通過した場合に限り永続化できる。Ctrl、Alt、Shift、Win等の修飾キー単独はショートカットとして記録しない。

禁止:
- SQLite
- Realtime Log
- 診断ログ
- テキストログ
- JSON/JSONLログ
- CSV
- debug print
- crash report
- telemetry
- その他、入力列を後から復元できる保存先

UI設定やJSON設定で「記録」が指定されても、このルールを必ず優先する。

### RULE-SEC-002 Realtime LogはDB記録対象だけ

Realtime Logへ流してよい入力は、最終的にDB記録対象として確定したショートカットイベントだけとする。

DisplayOnly、Ignore、記録禁止対象の修飾なし英数記号入力、除外アプリ、セキュリティ保護中、計測停止中の入力はRealtime Logへ送らない。

### RULE-SEC-003 パスワード・資格情報入力は完全抑止

パスワードまたは資格情報入力中と判定した場合は、次をすべて禁止する。

- Overlay表示
- DB記録
- Realtime Log
- 入力内容を含む診断
- 統計加算

password属性付き入力欄、Windowsログオン、ロック解除、資格情報入力、UAC資格情報、Windows Hello/PIN、RDP資格情報等を保護対象とする。

状態不明時は「記録する」より「記録しない」を優先する。

### RULE-SEC-004 セキュリティ判定を入力表示・記録より前に行う

Hook callback自体では重い処理を行わない既存原則を維持しつつ、Overlay表示・Realtime Log・DB writerへ渡すより前にSecurity Gateを通す。

## 2. データ最小化

保存対象は必要最小限とする。

- 正規化済みショートカット
- foreground executable basename
- 集計に必要な時刻
- 利用時間機能ON時の日次アプリ利用秒数

保存しない:
- 通常文字列
- ウィンドウタイトル
- URL
- ドキュメント名
- パスワード
- フォーカス中入力欄の文字列

## 3. Hook callback原則

Hook callback内で禁止:
- SQLite
- ファイルI/O
- UI更新
- UI Automationの重い問い合わせ
- ネットワーク
- 長時間Lock
- 大量文字列処理

callbackは軽量イベントをbounded queueへ渡すだけとする。

## 4. 設定よりセキュリティを優先する

設定ファイル・UI・旧バージョンの互換値がセキュリティルールと矛盾した場合、セキュリティルールを優先する。

## 5. 回帰テスト必須項目

各リリースで最低限確認する。

- 修飾なしの英字・数字・記号・テンキー入力が永続化されない
- 明示設定したFキー・システムキー・マウス・ホイールがSecurity Gate通過時だけ永続化される
- password入力欄でOverlayが出ない
- password入力欄でDBが増えない
- password入力欄でRealtime Logが増えない
- 除外アプリで入力を保存しない
- Local/Elevated経路の重複抑制
- Overlay click-through / focus非奪取
- Capture停止中のTray状態

## 6. 設定改ざん耐性

### RULE-SEC-006 記録禁止は設定ファイルで解除できない

修飾なしの英字、数字、記号、テンキー数字・演算子の記録禁止はhard-coded security invariantとして扱う。

- 記録禁止対象についてはUIに記録設定を出さない。
- JSONに記録指定を書いても無効。
- DB上の設定値を改ざんしても無効。
- migration時に旧設定が残っていても無効。
- backend runtimeで最終拒否する。

## 7. Integrity Level境界

### RULE-SEC-007 通常権限から高権限Foregroundを記録しない

ShortcutKeyCounterが通常権限で動作中に、Foreground Windowがより高いIntegrity Levelである場合、その入力はOverlay・DB・Realtime Log・入力統計へ一切流さない。

権限判定に失敗した場合もFail Closedとする。

ShortcutKeyCounter自身が昇格済みの場合でも、password / credential / secure desktop判定は必ず継続する。

UIAccessを使って通常権限から高権限UIへアクセスする方式を既定設計にしない。

## 8. Previewデータ

### RULE-SEC-008 SAMPLE previewを実データ経路へ流さない

Overlay設定プレビュー用 `S A M P L E` はPreview専用イベントとして扱う。

禁止:
- DB
- Realtime Log
- shortcut_daily
- CSV
- 診断ログ
- 利用統計

## 9. サンプルデータ

サンプルデータ生成は明示的なユーザー操作でのみ行う。
対象DB・件数・期間を確認後に実行する。
通常Capture経路へ偽入力を注入して生成してはならず、backendの検証用データ生成処理としてDB整合性を保って投入する。


## 10. 水平ホイール

### RULE-SEC-010 WheelLeft / WheelRightの単独記録

`WheelLeft` / `WheelRight` は修飾の有無を問わず、Security Gate通過後にKeyPolicyで記録可否を決定できる。修飾なしの場合も、利用者が明示的に記録を選択したときだけ永続化する。

単独ホイールは `RULE-UI-011` に従ってOverlayへ表示しない。表示可否と記録可否を混同しない。


## 11. Modifier-only / Wheel-only Overlay表示

### RULE-UI-011

Overlay表示について以下を強制する。

表示しない:
- Ctrl単独
- Alt単独
- Shift単独
- Win単独
- WheelUp単独
- WheelDown単独
- WheelLeft単独
- WheelRight単独

表示可能:
- 文字・数字・Fキー・通常キー単独
- MouseLeft / MouseRight / MouseMiddle / MouseX1 / MouseX2単独

修飾付きWheelはKeyPolicyに従う。

## 12. Overlay退避クリック

### RULE-UI-012 click-throughを破壊しない

Overlay退避操作を実装するためにOverlay Windowを通常のclickable windowへ変更してはならない。

グローバルMouse Eventのscreen coordinateとOverlay boundsを比較して退避操作を検知し、OSのクリックは背面アプリへ通す。

### RULE-UI-013 退避クリックをOverlay入力として表示しない

Overlay自身を移動するためのMouseLeftは、Overlayの入力キャップへ追加しない。


## 13. サンプルデータ生成

### RULE-DATA-013 本番DBへサンプルを書き込まない

サンプルデータの書き込み先は検証DBだけとする。
母集団として本番DBのapplication / shortcutを読み取ることは許可するが、本番DBへのINSERT/UPDATEは禁止する。

### RULE-DATA-014 サンプルでもSecurity Gateを迂回しない

60日分のshortcut sampleおよび前日分Realtime Log sampleを生成する際も、記録禁止対象の修飾なし英数記号入力・Security Gate禁止入力を生成してはならない。

### RULE-DATA-015 Realtime Log sampleで通常ログ仕様を弱めない

前日分Realtime Log sampleのために、通常Realtime Logの「DB記録対象イベントだけ」というルールを変更してはならない。

## 14. v0.1.4 キーマッピング検索

### RULE-SEC-016 検索Captureを通常入力経路へ流さない

「キーを入力して検索」がarmed中に取得した物理入力は、Security Gate通過後に検索処理で完全消費する。

禁止:
- Overlay
- SQLite
- Realtime Log
- shortcut_daily
- 統計加算
- 入力内容を含む診断ログ

検索結果には表示名ではなく内部のnormalized key identityを使用する。

## 15. v0.1.4 Overlay連打圧縮

### RULE-UI-017 Overlay Repeat CompressorとDB DuplicateGuardを分離する

Overlayの `×N` は物理入力回数を表す。DB DuplicateGuardの保存件数をOverlayカウンターへ流用してはならない。

- 同一normalized identityの700ms以内の連続入力を同一runとする。
- 1回目は通常表示、2回目は2入力分を表示、3回目から `×N` へ圧縮する。
- カウント更新時はカウンター部分だけをアニメーションし、既存キーキャップのIN/OUT clockを再起動しない。

## 16. v0.1.4 Overlay退避クリック

### RULE-UI-018 退避クリックは修飾付きでもShortcutKeyCounter内では完全破棄する

Overlay bounds内のMouseLeftが退避操作として成立した場合、Ctrl/Alt/Shift/Winが押されていても入力イベントとして扱わない。

禁止:
- DB
- Realtime Log
- Overlay入力キャップ
- shortcut_daily
- 統計加算

Windows側の元クリックは既存click-throughにより背面アプリへ届くことを維持する。

## 17. v0.1.4 使用率グラフ

### RULE-DATA-019 グラフは集計値だけをFrontendへ返す

使用率グラフのために詳細イベント全件をFrontendへ送ってはならない。SQLite側で期間bucketへ集計する。

- 今日: 時間
- 7日: 日
- 30日: 日
- 全期間: 月

系列色は内部identity単位で保存し、キー表示名変更後も色設定を維持できるようにする。
