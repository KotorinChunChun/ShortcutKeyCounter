# Clean Windows VM acceptance
1. Clean Windows 10/11 x64へinstallerを導入する。
2. 起動、通常Capture、Tray、自動起動を確認する。
3. Elevated ONでUACを承認し、管理者Notepad上のCtrl+Cを記録する。
4. v0.1.0を再インストール/更新してDBが保持されることを確認する。
5. アンインストール時にデータ削除未選択→再導入しDB保持を確認する。
6. 再度アンインストールしデータ削除を選択→`%LOCALAPPDATA%\ShortcutKeyCounter` 消去を確認する。
