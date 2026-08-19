# ShortcutKeyCounter 更新報告 2026-08-16

## 反映内容

- アプリ/Trayアイコンを白い立体 `C` 1キーへ変更。
- 作者を `KotoriChun`、GitHubを `https://github.com/KotorinChunChun/ShortcutKeyCounter` に設定。
- Sidebar下部のプライバシー文言を削除（Helpへ集約）。
- Helpのアプリ/作者/GitHubを見出し直下ではなくインデント表示。
- Rankingを2階層化。
  - 日刊ランキング
  - 累計キー別ランキング
  - アプリ別ランキング
  - リアルタイムログ
  - キー × アプリ見取り図
- キー/アプリのランキング行も親→内訳の2段階treeとして展開可能。
- キー × アプリのヒートマップ型マトリクスを追加。

## Elevated Capture不具合修正

### 旧問題

Elevated ON成功時に通常権限Hookを停止していたため、helper経路で入力が届かない状況では全計測が止まる構成だった。またElevated OFFでNamed Pipe reader `join()` を同期的に待つ経路があり、UI操作を長時間blockできた。

### 新方式

- 通常権限HookはElevated ON後も継続する。
- `ShortcutKeyCapture.exe` は追加Capture経路として動作する。
- 2経路の入力は同じCapture Workerへ集約し、既存の1秒DuplicateGuardで二重記録を抑制する。
- helper接続は15秒timeout。
- Elevated ON/OFF Tauri commandはblocking poolで実行する。
- OFF時は`CancelIoEx`でpending Named Pipe I/Oをcancelし、disconnect/joinはcleanup threadで実行する。
- helperが切断しても通常Captureは継続し、healthだけをdegraded/local fallbackへ変更する。

## 起動 / Single Instance

- Windowsログオン自動起動は `ShortcutKeyCounter.exe -hide`。
- `-hide` のときは起動時WebViewを生成しない。
- 引数なしの手動起動では起動時にUIを生成して表示する。
- Windows named Mutexをbackend初期化前に取得する。
- 2個目のprocessはHook/DBを起動せず、named activation Eventで既存processへ通知して終了する。
- 既存processはactivationを受けたら設定画面を表示する。

## Tray / WebView capture状態同期

- Capture ON/OFFのbackend stateは1つだけを使用。
- UI commandとTray menuのどちらから切り替えても同じ`AppState`を変更する。
- 変更後はTauri event `capture-state-changed` を発行し、開いているWebViewへ即時反映する。
- UIが閉じている場合、再生成時の`get_capture_status`で最新状態を取得する。

## Windowsで必ず行う回帰試験

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release -p shortcut-key-counter -p shortcut-key-capture
.\scripts\test-webview-lifecycle.ps1
```

ElevatedはUAC/管理者processが必要なため `docs/WINDOWS_BUILD_AND_ACCEPTANCE.md` の手動回帰手順も実施する。
