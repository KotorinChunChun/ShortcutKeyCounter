# OVL-11 オーバーレイをワンクリックでOFF

## 目的

ユーザーが設定画面を開かず、システムトレイからOverlay表示を1クリックでON/OFFできるようにする。

ユーザー要望の「チェックピン」は、本タスクでは**トレイのチェック項目**として実装する。

---

## 依存

OVL-01

---

## 仕様

トレイメニュー例:

```text
計測                 ✓
管理者権限アプリでも計測 ✓
オーバーレイ表示       ✓
------------------------
設定を開く
終了
```

クリック:

```text
Overlay ON -> OFF
Overlay OFF -> ON
```

Capture/DB計測は停止しない。

---

## 状態

Overlay enabledのSource of Truthを1つにする。

推奨:

```rust
pub struct OverlayControl {
    enabled: AtomicBool,
    sender: Sender<OverlayCommand>,
}
```

```rust
pub enum OverlayCommand {
    SetEnabled(bool),
    ReloadConfig,
}
```

---

## OFF処理

```rust
pub fn set_enabled(&self, enabled: bool) {
    self.enabled.store(enabled, Ordering::Release);

    let _ = self.sender.try_send(
        OverlayCommand::SetEnabled(enabled)
    );
}
```

Overlay thread:

```rust
match command {
    OverlayCommand::SetEnabled(false) => {
        state.items.clear();
        window.hide();
        stop_animation_timer();
    }

    OverlayCommand::SetEnabled(true) => {
        // 待機するだけ。
        // 過去入力は表示しない。
    }

    _ => {}
}
```

---

## Capture hot path

```rust
if overlay_control.is_enabled() {
    overlay_control.try_show(event);
}
```

enabled判定はAtomicBool等の低コスト読取。

DB処理には影響させない。

---

## Tray同期

トレイ項目ID例:

```rust
const TRAY_OVERLAY_TOGGLE_ID: &str = "overlay-toggle";
```

クリック:

```rust
let next = !overlay_control.is_enabled();
overlay_control.set_enabled(next);
update_tray_overlay_check(next);
persist_overlay_enabled(next);
emit_overlay_enabled_changed(next);
```

UI WebViewが閉じていても動作すること。

---

## 永続化

既存SQLite `settings.overlay_enabled` があるなら再利用する。

`overlay-theme.json.enabled`と二重管理しない。

推奨:

```text
SQLite settings.overlay_enabled = runtime feature switch
overlay-theme.json = appearance/layout config
```

OVL-01の`enabled`をJSONに入れた場合はOVL-13でSource of Truthを整理する。
**最終的には二重管理禁止。**

---

## UI event

```rust
app.emit("overlay-enabled-changed", next)?;
```

Frontend:

```ts
await listen<boolean>("overlay-enabled-changed", (event) => {
  setOverlayToggle(event.payload);
});
```

---

## テスト

- Overlay ON中にCtrl+C -> 表示
- tray click OFF
- 次のCtrl+C -> 非表示だがDB記録
- OFF時に通常A -> 非表示・DBなし
- tray click ON
- 次のA -> 表示
- UIを閉じてもtray toggle動作
- 再起動後状態復元

---

## 完了条件

- 1clickでON/OFF
- Capture/DBは止まらない
- OFF時は表示中Windowを即座にhide
- UIとtrayの表示状態同期
- WebView未生成でも動作

---

## 禁止

- Overlay OFFをCapture OFFと共用しない
- OFF時の過去ItemをON時に再表示しない
