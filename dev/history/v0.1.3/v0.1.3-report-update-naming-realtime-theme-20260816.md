# ShortcutKeyCounter 更新報告 2026-08-16（名称・リアルタイムログ・テーマ）

## 1. アプリ名称の統一

正式名称を `ShortcutKeyCounter` に統一した。

主な変更:

- Tauri productName: `ShortcutKeyCounter`
- メインEXE: `ShortcutKeyCounter.exe`
- Elevated helper: `ShortcutKeyCapture.exe`
- Cargo package: `shortcut-key-counter` / `shortcut-key-capture`
- ソースディレクトリ: `apps/shortcut-key-counter` / `apps/shortcut-key-capture`
- Windows Run値: `ShortcutKeyCounter`
- LocalAppData: `%LOCALAPPDATA%\ShortcutKeyCounter\`
- DB: `ShortcutKeyCounter.db`
- 診断ログ: `ShortcutKeyCounter_YYYYMMDD.log`
- キーマッピング設定: `ShortcutKeyCounter_key_mapping.json`
- CSV/DB backup/JSON exportのprefix: `ShortcutKeyCounter_*`
- Inno Setup: `ShortcutKeyCounter.iss`
- 配布ZIP: `ShortcutKeyCounter-v<version>-win-x64.zip`
- Window/Tray/Help/README等の表示名もすべて `ShortcutKeyCounter`

## 2. リアルタイムログをイベント駆動へ変更

従来の750msポーリングを廃止した。

処理フロー:

```text
Capture worker
  -> RealtimeLog.push()
  -> subscriber channelへsequence通知
  -> Tauri appが realtime-log-changed をemit
  -> WebViewがイベントを受信
  -> get_realtime_logs(after_sequence) を即時invoke
  -> ログ表を再描画
```

入力が短時間に集中して読み込み中になった場合は `realtimeReloadPending` を立て、現在の読み込み完了直後にもう一度差分を取得する。これによりinvokeを無制限に並列化せず、最新sequenceまで追従する。

UIを閉じてWebViewを破棄している間もring bufferはbackend側で最大500件保持する。再表示時には最新500件まで取得する。

## 3. ダーク/ライトテーマ

`settings` テーブルへ `theme` を追加した。

値:

- `dark`（既定）
- `light`

設定画面に「外観 > テーマ」を追加し、ダーク/ライトを切り替え可能とした。

- SQLiteへ永続化
- WebView再生成後も設定を復元
- localStorageにもミラーし、画面生成直後の配色切替を早める
- sidebar / panel / table / button / modal / toast / key / status等にライト用配色を定義

## 4. 静的検証

この環境で確認済み:

- TypeScript strict compile: PASS
- JSON parse: PASS
- TOML parse: PASS
- HTML IDとTypeScript参照の整合: PASS
- 旧アプリ名文字列検索: 0件
- リアルタイムログ旧750ms timer検索: 0件
- realtime-log-changed event/API/UI接続確認: PASS
- theme backend/API/UI接続確認: PASS

Rust/Cargo Windows toolchainはこの実行環境にないため、最終的な `cargo check / clippy / test / build` はWindows側で実行すること。
