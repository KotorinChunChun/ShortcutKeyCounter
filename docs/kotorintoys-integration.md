# KotorinToysからの管理

開発版はKotorinToys本体から起動・画面表示・状態確認・正常停止ができます。既存プロセスは二重起動せず、同じPIDの画面を再表示します。未保存のキーマッピング編集がある場合は停止を拒否するため、保存または破棄してから再操作してください。

- `--startup`：既存の`-hide`と同じく初期WebViewを作らず開始する。
- `--launch-origin host_manual|via_kotorintoys|direct_task`：管理用の起動経路。不正・欠落は`standalone`とする。
- `KotorinToys.<利用者SID>.shortcut-key-counter`：利用者専用ACL・同一Windowsセッション・ローカル接続限定の管理窓口。改行区切りのプロトコル2で`status`、`show`、`stop`を提供する。変更操作には現在の`expectedPid`が必要。
- 管理応答に入力、アプリ名、ウィンドウ名、統計、設定内容を含めない。入力保護・保存先・単独起動・トレイの終了操作は維持する。
- メインタイトルは`ShortcutKeyCounter - ver X.Y.Z`で、`WindowTitle::create`がCargo製品版から生成する。

現在公開しているポータブルZIPは本体対応Innoインストーラーではありません。本体からの自動ダウンロード・正式導入・更新は今回対応していません。既存のポータブル版からのデータ移動・削除や、Windows自動起動設定の変更も行いません。

## 管理連携の隔離テスト

Debugビルドに限り、`SHORTCUT_KEY_COUNTER_TEST_LOCALAPPDATA`へ絶対パスを渡すと、その配下に検証DB・設定を作成します。通常の保存先・Runキー同期を使用せず、入力Hook・オーバーレイを開始しません。Releaseビルドはこの変数を無視します。既存の通常プロセスがある場合は単一起動を優先するため、隔離試験は実行しないでください。
