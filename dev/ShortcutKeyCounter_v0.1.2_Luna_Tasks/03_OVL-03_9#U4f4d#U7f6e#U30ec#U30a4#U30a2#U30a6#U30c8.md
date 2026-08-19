# OVL-03 Overlay表示位置を9箇所から選択

## 目的

Foreground Windowのあるモニターの`rcWork`を基準に、3×3の9位置へOverlayを配置できるようにする。

---

## 依存

OVL-01

---

## 最初に探すコード

```powershell
rg -n "rcWork|MONITORINFO|MonitorFromWindow|bottom_margin|bottom-center|SetWindowPos|MoveWindow" crates apps
```

---

## 仕様

```text
top-left       top-center       top-right
middle-left    center           middle-right
bottom-left    bottom-center    bottom-right
```

基準は常に`rcMonitor`ではなく`rcWork`。

---

## Windows側の純粋関数を先に作る

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
```

### 計算関数

```rust
pub fn calculate_overlay_origin(
    work: WorkArea,
    size: Size,
    position: OverlayPosition,
    margin: i32,
) -> Point {
    let left = work.left + margin;
    let center_x = work.left + ((work.right - work.left - size.width) / 2);
    let right = work.right - margin - size.width;

    let top = work.top + margin;
    let center_y = work.top + ((work.bottom - work.top - size.height) / 2);
    let bottom = work.bottom - margin - size.height;

    match position {
        OverlayPosition::TopLeft => Point { x: left, y: top },
        OverlayPosition::TopCenter => Point { x: center_x, y: top },
        OverlayPosition::TopRight => Point { x: right, y: top },
        OverlayPosition::MiddleLeft => Point { x: left, y: center_y },
        OverlayPosition::Center => Point { x: center_x, y: center_y },
        OverlayPosition::MiddleRight => Point { x: right, y: center_y },
        OverlayPosition::BottomLeft => Point { x: left, y: bottom },
        OverlayPosition::BottomCenter => Point { x: center_x, y: bottom },
        OverlayPosition::BottomRight => Point { x: right, y: bottom },
    }
}
```

---

## DPI

既存Per-monitor DPIスケールをそのまま使う。

```rust
let margin = scale_px(config.margin_px, monitor_dpi);
let overlay_size = Size {
    width: scale_px(layout_width, monitor_dpi),
    height: scale_px(layout_height, monitor_dpi),
};
```

### 注意

`margin_px`を2回スケールしない。

---

## Win32適用例

```rust
let origin = calculate_overlay_origin(
    work_area,
    overlay_size,
    config.fixed_position,
    margin,
);

unsafe {
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        origin.x,
        origin.y,
        overlay_size.width,
        overlay_size.height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    )?;
}
```

既存のWrapperがある場合は直接APIを増やさずWrapperへ統合する。

---

## Unit Test

```rust
#[test]
fn bottom_center_matches_existing_position() {
    let work = WorkArea {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    let size = Size {
        width: 600,
        height: 200,
    };

    let p = calculate_overlay_origin(
        work,
        size,
        OverlayPosition::BottomCenter,
        64,
    );

    assert_eq!(p.x, 660);
    assert_eq!(p.y, 776);
}

#[test]
fn top_right_respects_non_zero_monitor_origin() {
    let work = WorkArea {
        left: 1920,
        top: -200,
        right: 4480,
        bottom: 1240,
    };
    let size = Size { width: 500, height: 200 };

    let p = calculate_overlay_origin(
        work,
        size,
        OverlayPosition::TopRight,
        50,
    );

    assert_eq!(p.x, 3930);
    assert_eq!(p.y, -150);
}
```

---

## 完了条件

- 9位置すべて計算可能
- デフォルト`bottom-center`がv0.1.1と概ね同一
- 負座標モニター対応
- タスクバーを除いた`rcWork`使用
- DPI対応維持
- focus/click-through維持

---

## 禁止

- 仮想デスクトップ全体を基準にしない
- Primary monitor固定にしない
- Overlayを全画面化しない
