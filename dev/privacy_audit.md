# Privacy audit
DBに保存してよい値: timestamp、normalized shortcut、executable basename、count。
保存禁止: normal text、window title、URL、document name、clipboard、full executable path。
`shortcut_events`, `shortcut_daily`, logs, CSV exportを確認し、禁止情報が存在しないことを受入時に確認する。
