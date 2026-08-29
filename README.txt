ShortcutKeyCounter v0.1.4
========================

Windowsのバックグラウンドに常駐し、入力されたショートカットキーをキー別、アプリ別、時間軸でローカル集計する Rust + Tauri 2 アプリです。
リアルタイムログ、ランキング、使用率グラフ、ヒートマップ、CSV出力、ショートカットキービジュアライズ機能を搭載しています。

Download:
  https://github.com/KotorinChunChun/ShortcutKeyCounter/releases

User Guide:
  https://github.com/KotorinChunChun/ShortcutKeyCounter/blob/feature/v0.1.5/docs/users-guide.md

Start:
  1. ZIPを展開します。
  2. ShortcutKeyCounter.exeとShortcutKeyCapture.exeを同じフォルダーに置きます。
  3. ShortcutKeyCounter.exeを実行します。
  4. 画面を閉じても通知領域で計測を継続します。完全終了は通知領域メニューの「終了」を選びます。

v0.1.4 highlights:
- 修飾なし単独入力はkeyboard/mouse/wheelを問わず永続記録禁止。通常mouse clickだけOverlay表示可。
- Security GateはOverlay/Realtime Log/DBより前でFail Closed。
- Overlay設定combo/固定monitorの操作不具合を修正。
- DuplicateGuard 0.0〜10.0秒、詳細event保持1〜365日をruntime設定化。
- Key Mappingへ独立scroll、一括変更、物理キー検索専用Captureを追加。検索入力は記録しない。
- Rankingをキー別/アプリ別左右2ペインへ統合し、期間比較、対象アプリ再集計、平均/構成比/3-state sortに対応。
- Usage Graphを追加し、SQLite bucket集計・上位5系列・色変更/identity別保存に対応。
- Overlayは700ms連打判定、3回目から `×N` へ圧縮。count専用animationとRepeat Counter Themeを追加。
- Sample dataはTest DBだけへ、前日を最新日とする60日・約10,000event + app usageを生成。
- Overlay退避clickは修飾付きでも内部記録せず、click-throughを維持。

Build:
  pwsh -ExecutionPolicy Bypass -File .\scripts\build-release.ps1 -SkipInstaller

Data:
  %LOCALAPPDATA%\ShortcutKeyCounter\ShortcutKeyCounter.db
  %LOCALAPPDATA%\ShortcutKeyCounter\ShortcutKeyCounter.test.db
  %LOCALAPPDATA%\ShortcutKeyCounter\overlay_key_labels.json
  %LOCALAPPDATA%\ShortcutKeyCounter\overlay_key_policy.json
  %LOCALAPPDATA%\ShortcutKeyCounter\overlay-theme.json

重要:
修飾なし単独入力の記録禁止はRuntime側のSecurity invariantです。UI/JSONを変更しても解除できません。
計測・集計・CSV機能はネットワーク通信を行わず、計測データを外部へ自動送信しません。
ただし、単独キー表示中の画面共有などでは入力が画面へ公開される可能性があります。パスワードや機密情報を入力する前に、画面共有、オーバーレイ、計測を停止してください。

CSV共有:
  CSVはExcel等で分析できます。共有は任意です。共有前に内容を確認・匿名化し、SQLite DBや未確認の詳細イベントCSVを公開しないでください。
