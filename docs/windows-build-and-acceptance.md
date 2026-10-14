# Windows build / acceptance

## 0. 2026-08-16 DB lock型エラー回帰確認

`apps/shortcut-key-counter/src/tauri_commands.rs` に `&db_lock(&state)?` が残っていないことを確認します。DBを使うTauri commandは `let db = db_lock(&state)?;` でguardを束縛してから `command_api` へ `&db` を渡します。

`dev/scripts/test-release-ShortcutKeyCounter-all.ps1` はこの禁止パターンを自動検査し、その後 `cargo check --workspace --all-targets --all-features` を実行します。E0308（`expected Connection, found MutexGuard`）が再発した場合は検証を中断してください。


## Build prerequisites

- Windows 10/11 x64
- Rust stable (MSVC target)
- Visual Studio Build Tools with Desktop development with C++
- Microsoft Edge WebView2 Runtime
- Node.js + TypeScript (`tsc`)
- Inno Setup 6 (installerを作る場合)

## Release build

```powershell
Set-ExecutionPolicy -Scope Process Bypass
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-release-ShortcutKeyCounter-all.ps1
```

この処理は自動品質検証とReleaseビルドまでを行います。ZIP生成と外部公開は別工程です。

## Mandatory acceptance

1. 起動直後に計測ON。
2. `Ctrl+C`, `Alt+Tab`, `Win+E`, `Ctrl+Shift+P` が記録される。
3. `A` や `Shift+A` は記録されない。
4. 長押しのkey repeatで1押下が過剰カウントされない。
5. 同じ正規化shortcutを1秒以内に別アプリで入力しても2件目を記録しない。
6. DBには実行ファイル名のみ入り、ウィンドウタイトル・URL・入力文章が存在しない。
7. KeePass / Bitwarden等のdefault除外上では記録されない。
8. UIで除外を追加・解除した直後から再起動なしで反映される。
9. 詳細ショートカット履歴は起動時に7日より古いものが削除される。日次集計は残る。
10. ダッシュボード、ランキング、アプリ別、詳細の各画面が実DB値で更新される。
11. 計測OFF中に入力してもDB件数が増えず、再起動後はONに戻る。
12. CSVがUTF-8 BOM付きでDownloadsへ出力される。
13. DBバックアップがSQLite Backup API経由で作成され、開ける。
14. 全削除で `shortcut_events` / `shortcut_daily` のみ消え、設定と除外は残る。
15. 自動起動ON/OFFがWindowsログオン起動へ反映され、登録commandが `ShortcutKeyCounter.exe -hide` になる。
16. Elevated ON時にUACが表示され、`ShortcutKeyCapture.exe`だけが昇格する。通常権限Hookは停止せず計測を継続する。
17. Elevated helper切断時も通常権限Captureが途切れず、healthがlocal fallback/degradedへ変化する。
18. Elevated OFF操作がフリーズせず短時間で戻り、helperが終了する。
19. スリープ/休止復帰後に計測が継続する。
20. ×でウィンドウを閉じてもTray常駐し、Trayから再表示できる。
21. Tray/UIどちらで計測ON/OFFしても相互に即時表示反映する。
22. `-hide` 付き起動はUI/WebView2を作らず、引数なし起動はUIを表示する。
23. 二重起動時はnamed Mutexで新規backendを作らず、既存processの設定画面を表示する。
24. ZIPにはDBを含めない。
25. Inno Setupアンインストール時、ローカルデータ削除チェックを未選択ならDBを保持する。

## UI WebView lifecycle acceptance (required)

The main UI is created on normal manual launch, or lazily after a `-hide` logon launch. Closing the window destroys the native window and WebView2; it must not call `hide()` as a substitute. The capture/runtime process and tray icon remain alive.

After a release build, run:

```powershell
pwsh -ExecutionPolicy Bypass -File .\dev\scripts\test-exe-ShortcutKeyCounter-webview-lifecycle.ps1
```

PASS requires all of the following:

- `ShortcutKeyCounter.exe` remains alive after the main window is closed.
- The `ShortcutKeyCounter` native window no longer exists.
- Descendant `msedgewebview2.exe` processes reach zero after closing the UI.
- Starting a second instance hands off to the existing process and recreates the UI/WebView2.
- Closing the recreated UI again releases WebView2 a second time.
- Manually double-click the notification-area icon once and confirm the same UI recreation path works.

This test distinguishes **destroying the WebView** from merely hiding the window.

## New v0.1.0 UI acceptance

- Application/tray icon is a close-up white 3D `C` single-key design.
- Sidebar header shows `v0.1.0` instead of `Local usage analytics`.
- Realtime Log shows normalized shortcut, executable basename, and decision only; normal text never appears.
- Key Mapping can disable/enable supported keyboard and mouse main inputs and immediately affects capture.
- Key Mapping persists to `%LOCALAPPDATA%\ShortcutKeyCounter\ShortcutKeyCounter_key_mapping.json` and exports a timestamped JSON to Downloads.
- CSV export, event CSV export, DB backup, and key-mapping JSON success toasts include a `フォルダーを開く` action.
- Help page shows version, author `KotoriChun`, GitHub repository link, privacy note, and basic usage.
- Ranking navigation is two-level: Daily / cumulative key / application / realtime / key×application matrix.


## Elevated capture regression procedure

1. Elevated OFFで `Ctrl+C` を通常アプリ上で入力し、Realtime Log/DBに記録されることを確認。
2. `管理者権限アプリでも計測` をONにしUACを承認。通常アプリ上の `Ctrl+C` が引き続き記録されることを確認。
3. 管理者として起動したNotepad/Windows Terminal上で対象shortcutが記録されることを確認。
4. Elevated ONのまま設定をOFFへ戻す。操作中にWebViewがフリーズせず、設定がOFFへ戻ることを確認。
5. 再びONにし、Task Managerから `ShortcutKeyCapture.exe` を終了。通常アプリの計測が継続し、設定画面の入力取得経路が縮退表示になることを確認。

## Startup / single-instance regression procedure

```powershell
.\target\release\ShortcutKeyCounter.exe -hide
```

- `ShortcutKeyCounter.exe` は存在するが `ShortcutKeyCounter` windowと子 `msedgewebview2.exe` は存在しないこと。
- その状態で引数なしの `ShortcutKeyCounter.exe` を起動すると、2個目は終了し、既存processに設定画面が表示されること。
- 一度すべて終了し、引数なしで最初に起動するとUIが表示されること。

## v0.1.1 Shortcut Overlay acceptance (required)

### Build / source gate

`dev/scripts/test-release-ShortcutKeyCounter-all.ps1` が以下をPASSすること。

- Overlay default stable display duration is `1840ms`; IN/OUT animation timing remains independently configurable
- maximum stack = 3
- Local/Elevated source dedup = 50ms
- Elevated inputは専用 `try_send_elevated` 経路を通る
- `WS_EX_NOACTIVATE`, `WM_NCHITTEST`, `HTTRANSPARENT` が維持される
- Overlay通知がDB用 `duplicate_guard.should_accept` より前にある
- Overlay subsystem内にWebView/Tauri window依存を追加していない

### Basic display

1. Settingsの「ショートカットオーバーレイ」をONにする。
2. 通常アプリ上で `Ctrl+C` を入力する。
3. Foreground Windowがあるモニターの作業領域中央下にキーが表示されること。
4. Overlay表示によってForeground Window/keyboard focusがShortcutKeyCounterへ移らないこと。
5. Overlay上をクリックしても下のアプリへクリックが届くこと。
6. 修飾なし`A`等はOverlay表示可能だがDB/Realtime Logへ永続化されないこと。Modifier-onlyとwheel-onlyはOverlayに表示されないこと。wheel-onlyは明示的に記録設定した場合だけDB/Realtime Logへ永続化できること。
7. 除外アプリ上では表示されないこと。

### Timing / animation

ストップウォッチまたは60fps以上の画面録画で確認する。

- 入力直後から約160msでSlide/Fade Inが完了する。
- 既定のstable表示は1840msを基準とする。
- Fade Outはstable表示時間とは別設定として動作する。
- 既存ユーザーの明示設定値はmigrationで勝手に+1000msしない。
- Fade Outが長く尾を引かず、急に消える印象にもならないこと。

### Consecutive input / stack

次を約0.2～0.4秒間隔で入力する。

```text
Ctrl+C
Ctrl+V
Ctrl+S
```

PASS:

- 最新3件が表示される。
- 新規itemが下から入る。
- 既存itemが滑らかに上へ移動する。
- 各itemのTTLは独立している。

続けて4件目 `Ctrl+Z` を入力し、最新3件だけになることを確認する。

### DB DuplicateGuard separation

同一アプリで `Ctrl+C` を0.3秒程度の間隔で2回物理入力する。

PASS:

- Overlayでは `Ctrl+C` が2回の入力として表示される。
- 既存DB仕様により1秒以内の2件目はDBへ重複記録されない。

Overlay専用表示とDB統計のduplicate policyが混同されていないことを確認する。

### Elevated source dedup

1. 「管理者権限アプリでも計測」をONにする。
2. Local/Helperの両経路で取得可能なショートカットを入力する。
3. Overlayが同じ物理入力を2重表示しないこと。
4. 管理者権限アプリで対象shortcutを入力し、Overlayが1件表示されること。
5. Elevated OFFに戻してもOverlayと通常Captureが継続すること。

### Multi-monitor / DPI

- 100% scale monitorで入力して中央下表示を確認。
- 125%/150%等、異なるDPIのmonitorへForeground Windowを移して入力する。
- Overlayがそのmonitorへ移動すること。
- キーの物理的な見た目サイズが不自然に変化しないこと。
- タスクバー領域へ重ならず `rcWork` 内に収まること。

### JSON / SVG customization

Settingsから以下を確認する。

1. `overlay_key_labels.json` を開く。
2. 例として `Ctrl` の表示名を `CTRL` に変更する。
3. 「再読み込み」を実行する。
4. DB内部値を変えずOverlayだけ `CTRL + C` 表示になること。
5. `overlay-theme.json` の許容範囲内の値を変更してreloadする。
6. `overlay/default/*.svg` を変更してreloadし、キー外観が変わること。
7. JSONを一時的に不正形式にし、アプリ本体/Captureが停止せずdefaultへfallbackすること。
8. ユーザーの壊れたJSONが勝手に削除・上書きされないこと。

### ON / OFF

- Overlay OFFではshortcutのDB記録は継続するがOverlayは表示されない。
- Overlay ONへ戻すと再起動なしで表示が復帰する。
- 設定はアプリ再起動後も保持される。

### Performance

Task Manager / Process Explorer等で確認する。

Idle（Overlay itemなし）:

- animation用busy loopが発生しない。
- CPU使用率が継続上昇しない。
- WebView2 processがOverlayのために追加生成されない。

Rapid input:

- `Ctrl+C`, `Ctrl+V`, `Ctrl+S` 等を連続入力してもkeyboard/mouse hookに体感遅延がない。
- DB記録が停止しない。
- Overlay queue saturationが恒常的に発生しない。
- 長時間使用でOverlay bitmap cache / memoryが増え続けない。



## v0.1.3 mandatory security / data / overlay acceptance

1. 修飾なしの英字、数字、OEM記号、テンキー数字／演算子を入力してもSQLite/Realtime Log/診断へ残らない。
2. `overlay_key_policy.json`を直接改ざんして、記録禁止対象の修飾なし英数記号入力をrecord指定してもDBへ入らない。Fキー・システムキー・MouseLeft/Right/Middle/X1/X2・WheelUp/Down/Left/Rightは明示的にrecord指定し、Security Gate通過時にDB/Realtime Logへ入る。
3. password属性付き入力欄へfocus中はOverlay/DB/Realtime Log/入力統計が増えない。通常Webページにpassword欄が存在するだけでは停止しない。
4. ShortcutKeyCounterを通常権限で動かし、高Integrity Level foregroundへ切替すると入力処理が停止する。権限判定失敗時も記録しない。
5. Realtime Logは実際にDBへ記録されたshortcutだけを表示する。
6. Ctrl/Alt/Shift/Win単独、およびWheelUp/Down/Left/Right単独はOverlayへ表示しない。文字/Fキー/通常mouse click単独はOverlay表示可能。
7. Overlay上のMouseLeftは背面applicationへ通りつつOverlayを反対位置へ退避し、その退避click自身はShortcutKeyCounterの統計/Overlayへ追加しない。
8. active-window回避modeは許可した1〜9位置だけを候補にし、非重複を優先、全候補重複時はoverlap面積最小を選ぶ。
9. 高速単独キー入力はWork Area幅まで拡張し、超過時はkey単位で折返して切れない。既存tokenは再animationせず新規tokenだけIN animationする。
10. Overlay設定保存後、S A M P L E previewが新設定で表示され、DB/Realtime Log/統計には入らない。
11. Production/Test DB切替後、capture/statistics/export/backup/data managementが同じactive DBを使い、Test中は常時badgeで識別できる。
12. Sample生成はTest DBでのみ可能。本番DBのdistinct application/shortcutを母集団としてTest DBだけへ約10,000eventと利用時間sampleを書き込む。
13. 「前日以前を削除」で`shortcut_events`/`shortcut_daily`/`application_usage_daily`が同じ基準日で削除される。
14. 利用時間ON時、keyboard/mouseの最終入力が1分以上ない間は加算停止し、入力再開後に復帰する。Security Gate中でも利用時間自体は継続する。
15. fixed monitorを選択すると該当monitorへ識別previewが出る。保存monitor切断時はprimaryへfallbackし、設定identityは破棄しない。
16. Trayは記録中=通常C、正常停止=赤斜線、障害=赤×を区別し、tooltipに理由とActive DBを表示する。
17. Light/Darkを往復し、sidebar/card/table/form/dialog/filter/selected/disabledまで判読可能である。
18. Windows実機でmulti-monitor/DPI、horizontal wheel、Elevated helper、sleep/lock復帰を回帰確認する。

## v0.1.4 acceptance

v0.1.4の追加受入項目は [v0.1.4 Windows受入試験チェックリスト](../dev/history/v0.1.4/v0.1.4-checklist-windows-acceptance.md) を正とする。特に以下はrelease必須。

- DuplicateGuard runtime値（0.0〜10.0秒）とretention（1〜365日）。
- fixed monitor draft選択、切断済みmonitor ID、Light theme 9-position selected state。
- Key Mapping physical-key search inputがOverlay/DB/Realtime Log/statisticsへ一切流れないこと。
- RankingのToday同時刻比較、application filter再集計、average/ratio/comparison、3-state sort。
- Usage Graphのhour/day/month SQLite aggregation、top-5初期ON、identity別color persistence。
- Overlay Repeat Compressor: 700ms、3回目から`×N`、count-only animation、DB DuplicateGuardとの独立。
- modifier付きOverlay retreat clickもShortcutKeyCounter内部へ記録しないこと。
- Test DB sample: 前日を終端に60日、約10,000event、60日app usage、前日Realtime Log sample。
