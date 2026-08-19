//! Lightweight native layered window used by the shortcut overlay.
//!
//! This module intentionally owns HWND/HDC resources on the overlay thread only. The public
//! values crossing threads are plain integers/structs, so non-Send Win32 handle wrappers never
//! leave the thread that created them.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

use shortcut_common::overlay::OverlayPosition;
use thiserror::Error;
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{
    COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP,
    HDC, HGDIOBJ, MONITORENUMPROC, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetForegroundWindow, GetWindowRect, PeekMessageW, RegisterClassW, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TranslateMessage, UpdateLayeredWindowIndirect, GWLP_HWNDPARENT,
    HTTRANSPARENT, HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    UPDATELAYEREDWINDOWINFO, WM_NCHITTEST, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const OVERLAY_CLASS_NAME: windows::core::PCWSTR = w!("ShortcutKeyCounterOverlayWindow");
const AC_SRC_OVER_VALUE: u8 = 0;
const AC_SRC_ALPHA_VALUE: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlayMonitorMetrics {
    pub work_left: i32,
    pub work_top: i32,
    pub work_right: i32,
    pub work_bottom: i32,
    pub dpi: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayMonitorInfo {
    /// Persistent selection key. Windows display device name is more stable than a UI index.
    pub id: String,
    /// Human-readable label shown by the settings UI.
    pub label: String,
    pub work_left: i32,
    pub work_top: i32,
    pub work_right: i32,
    pub work_bottom: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// rcWork内のOverlay原点を計算する純粋関数。
pub fn calculate_overlay_origin(
    work: WorkArea,
    size: Size,
    position: OverlayPosition,
    margin: i32,
) -> Point {
    let margin = margin.max(0);
    let left = work.left.saturating_add(margin);
    let center_x = work.left + (work.right - work.left - size.width) / 2;
    let right = work.right - margin - size.width;
    let top = work.top.saturating_add(margin);
    let center_y = work.top + (work.bottom - work.top - size.height) / 2;
    let bottom = work.bottom - margin - size.height;

    match position {
        OverlayPosition::TopLeft => Point { x: left, y: top },
        OverlayPosition::TopCenter => Point {
            x: center_x,
            y: top,
        },
        OverlayPosition::TopRight => Point { x: right, y: top },
        OverlayPosition::MiddleLeft => Point {
            x: left,
            y: center_y,
        },
        OverlayPosition::Center => Point {
            x: center_x,
            y: center_y,
        },
        OverlayPosition::MiddleRight => Point {
            x: right,
            y: center_y,
        },
        OverlayPosition::BottomLeft => Point { x: left, y: bottom },
        OverlayPosition::BottomCenter => Point {
            x: center_x,
            y: bottom,
        },
        OverlayPosition::BottomRight => Point {
            x: right,
            y: bottom,
        },
    }
}

impl OverlayMonitorMetrics {
    pub fn scale(self) -> f32 {
        self.dpi.max(96) as f32 / 96.0
    }
}

#[derive(Debug, Error)]
pub enum OverlayWindowError {
    #[error("overlay window class registration failed")]
    ClassRegistration,
    #[error("overlay window creation failed: {0}")]
    WindowCreation(#[from] windows::core::Error),
    #[error("overlay memory DC creation failed")]
    MemoryDc,
    #[error("overlay DIB creation failed: {0}")]
    DibCreation(windows::core::Error),
    #[error("overlay monitor information could not be resolved")]
    MonitorInfo,
    #[error("overlay pixel buffer size is invalid")]
    InvalidBuffer,
    #[error("UpdateLayeredWindow failed")]
    UpdateFailed,
}

pub struct OverlayWindow {
    hwnd: HWND,
    surface: Option<DibSurface>,
    visible: bool,
}

impl OverlayWindow {
    pub fn create() -> Result<Self, OverlayWindowError> {
        let module =
            unsafe { GetModuleHandleW(None) }.map_err(OverlayWindowError::WindowCreation)?;
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSW {
            lpfnWndProc: Some(overlay_window_proc),
            hInstance: instance,
            lpszClassName: OVERLAY_CLASS_NAME,
            ..Default::default()
        };
        let atom = unsafe { RegisterClassW(&class) };
        if atom == 0 {
            // One OverlayWindow is created per process. A zero atom here means the class could not
            // be registered, rather than a legitimate second registration in this process.
            return Err(OverlayWindowError::ClassRegistration);
        }

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOPMOST,
                OVERLAY_CLASS_NAME,
                w!("ShortcutKeyCounter Overlay"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            )
        }
        .map_err(OverlayWindowError::WindowCreation)?;

        Ok(Self {
            hwnd,
            surface: None,
            visible: false,
        })
    }

    pub fn active_monitor_metrics(&self) -> Result<OverlayMonitorMetrics, OverlayWindowError> {
        let foreground = unsafe { GetForegroundWindow() };
        self.monitor_metrics_for_foreground(foreground)
    }

    pub fn foreground_window_rect(&self) -> Result<WorkArea, OverlayWindowError> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return Err(OverlayWindowError::MonitorInfo);
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(foreground, &mut rect) }
            .map_err(OverlayWindowError::WindowCreation)?;
        Ok(WorkArea {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    pub fn fixed_monitor_metrics(
        &self,
        monitor_id: &str,
    ) -> Result<OverlayMonitorMetrics, OverlayWindowError> {
        if let Some(monitor) = enumerate_monitors()
            .into_iter()
            .find(|monitor| monitor.id.eq_ignore_ascii_case(monitor_id))
        {
            return Ok(OverlayMonitorMetrics {
                work_left: monitor.work_left,
                work_top: monitor.work_top,
                work_right: monitor.work_right,
                work_bottom: monitor.work_bottom,
                // A monitor without a foreground HWND has no reliable per-monitor DPI API in
                // this dependency set.  96 is the safe logical fallback; active-window mode
                // keeps the existing per-window DPI behavior.
                dpi: 96,
            });
        }
        self.primary_monitor_metrics()
    }

    pub fn primary_monitor_metrics(&self) -> Result<OverlayMonitorMetrics, OverlayWindowError> {
        let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Err(OverlayWindowError::MonitorInfo);
        }
        Ok(OverlayMonitorMetrics {
            work_left: info.rcWork.left,
            work_top: info.rcWork.top,
            work_right: info.rcWork.right,
            work_bottom: info.rcWork.bottom,
            dpi: 96,
        })
    }

    pub fn active_window_metrics(
        &self,
    ) -> Result<(isize, OverlayMonitorMetrics), OverlayWindowError> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return Err(OverlayWindowError::MonitorInfo);
        }
        let mut client = RECT::default();
        unsafe { GetClientRect(foreground, &mut client) }
            .map_err(OverlayWindowError::WindowCreation)?;
        let mut origin = POINT { x: 0, y: 0 };
        if !unsafe { ClientToScreen(foreground, &mut origin) }.as_bool() {
            return Err(OverlayWindowError::MonitorInfo);
        }
        let dpi = unsafe { GetDpiForWindow(foreground) }.max(96);
        Ok((
            foreground.0 as isize,
            OverlayMonitorMetrics {
                work_left: origin.x,
                work_top: origin.y,
                work_right: origin.x + (client.right - client.left),
                work_bottom: origin.y + (client.bottom - client.top),
                dpi,
            },
        ))
    }

    /// Overlayをデスクトップ最前面または対象ウィンドウ所有のポップアップへ切り替える。
    pub fn set_display_target(
        &mut self,
        target_window: Option<isize>,
        desktop_mode: bool,
    ) -> Result<(), OverlayWindowError> {
        let owner = target_window
            .map(|value| HWND(value as *mut c_void))
            .unwrap_or_default();
        unsafe {
            let _ = SetWindowLongPtrW(self.hwnd, GWLP_HWNDPARENT, owner.0 as isize);
            let insert_after = if desktop_mode { HWND_TOPMOST } else { HWND_TOP };
            SetWindowPos(
                self.hwnd,
                Some(insert_after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
            .map_err(OverlayWindowError::WindowCreation)?;
            if !desktop_mode {
                SetWindowPos(
                    self.hwnd,
                    Some(HWND_NOTOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
                .map_err(OverlayWindowError::WindowCreation)?;
            }
        }
        Ok(())
    }

    fn monitor_metrics_for_foreground(
        &self,
        foreground: HWND,
    ) -> Result<OverlayMonitorMetrics, OverlayWindowError> {
        let monitor = unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Err(OverlayWindowError::MonitorInfo);
        }
        let dpi = if foreground.0.is_null() {
            96
        } else {
            unsafe { GetDpiForWindow(foreground) }.max(96)
        };
        Ok(OverlayMonitorMetrics {
            work_left: info.rcWork.left,
            work_top: info.rcWork.top,
            work_right: info.rcWork.right,
            work_bottom: info.rcWork.bottom,
            dpi,
        })
    }

    /// `rgba_premultiplied` is tiny-skia style premultiplied RGBA, top-down.
    pub fn present_rgba(
        &mut self,
        rgba_premultiplied: &[u8],
        width: u32,
        height: u32,
        x: i32,
        y: i32,
    ) -> Result<(), OverlayWindowError> {
        if width == 0
            || height == 0
            || rgba_premultiplied.len() != width as usize * height as usize * 4
        {
            return Err(OverlayWindowError::InvalidBuffer);
        }
        self.ensure_surface(width, height)?;
        let surface = self
            .surface
            .as_mut()
            .ok_or(OverlayWindowError::InvalidBuffer)?;
        surface.copy_from_rgba(rgba_premultiplied);

        let destination = POINT { x, y };
        let source = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: width as i32,
            cy: height as i32,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER_VALUE,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA_VALUE,
        };
        let info = UPDATELAYEREDWINDOWINFO {
            cbSize: size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
            hdcDst: HDC::default(),
            pptDst: &destination,
            psize: &size,
            hdcSrc: surface.hdc,
            pptSrc: &source,
            crKey: COLORREF(0),
            pblend: &blend,
            dwFlags: ULW_ALPHA,
            prcDirty: ptr::null(),
        };

        if !unsafe { UpdateLayeredWindowIndirect(self.hwnd, &info) }.as_bool() {
            return Err(OverlayWindowError::UpdateFailed);
        }
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                width as i32,
                height as i32,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
            .map_err(OverlayWindowError::WindowCreation)?;
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        self.visible = true;
        Ok(())
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }

    pub fn pump_messages(&self) {
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn ensure_surface(&mut self, width: u32, height: u32) -> Result<(), OverlayWindowError> {
        let matches = self
            .surface
            .as_ref()
            .is_some_and(|surface| surface.width == width && surface.height == height);
        if !matches {
            self.surface = Some(DibSurface::new(width, height)?);
        }
        Ok(())
    }
}

/// 接続中のモニターをWindowsのデバイス名で列挙する。デバイス名は設定ファイルへ保存しても
/// 再起動後に同一モニターを選びやすいよう、表示順ではなく`\\.\DISPLAYn`を利用する。
pub fn available_monitors() -> Vec<OverlayMonitorInfo> {
    enumerate_monitors()
}

fn enumerate_monitors() -> Vec<OverlayMonitorInfo> {
    let mut monitors = Vec::new();
    let data = LPARAM((&mut monitors as *mut Vec<OverlayMonitorInfo>) as isize);
    let callback: MONITORENUMPROC = Some(enumerate_monitor_callback);
    let _ = unsafe { EnumDisplayMonitors(None, None, callback, data) };
    monitors
}

unsafe extern "system" fn enumerate_monitor_callback(
    monitor: windows::Win32::Graphics::Gdi::HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = &mut *(data.0 as *mut Vec<OverlayMonitorInfo>);
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
        return BOOL(1);
    }
    let id = String::from_utf16_lossy(&info.szDevice)
        .trim_end_matches('\0')
        .to_owned();
    let width = info.monitorInfo.rcMonitor.right - info.monitorInfo.rcMonitor.left;
    let height = info.monitorInfo.rcMonitor.bottom - info.monitorInfo.rcMonitor.top;
    let label = format!("{} ({}×{})", id, width, height);
    monitors.push(OverlayMonitorInfo {
        id,
        label,
        work_left: info.monitorInfo.rcWork.left,
        work_top: info.monitorInfo.rcWork.top,
        work_right: info.monitorInfo.rcWork.right,
        work_bottom: info.monitorInfo.rcWork.bottom,
    });
    BOOL(1)
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        self.surface = None;
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

struct DibSurface {
    hdc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl DibSurface {
    fn new(width: u32, height: u32) -> Result<Self, OverlayWindowError> {
        let screen_dc = unsafe { GetDC(None) };
        let hdc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
        unsafe {
            let _ = ReleaseDC(None, screen_dc);
        }
        if hdc.0.is_null() {
            return Err(OverlayWindowError::MemoryDc);
        }

        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: width.saturating_mul(height).saturating_mul(4),
            ..Default::default()
        };
        let info = BITMAPINFO {
            bmiHeader: header,
            ..Default::default()
        };
        let mut bits: *mut c_void = ptr::null_mut();
        let bitmap =
            unsafe { CreateDIBSection(Some(hdc), &info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .map_err(OverlayWindowError::DibCreation)?;
        if bits.is_null() {
            unsafe {
                let _ = DeleteDC(hdc);
            }
            return Err(OverlayWindowError::InvalidBuffer);
        }
        let previous = unsafe { SelectObject(hdc, bitmap.into()) };
        Ok(Self {
            hdc,
            bitmap,
            previous,
            bits: bits.cast::<u8>(),
            width,
            height,
        })
    }

    fn copy_from_rgba(&mut self, rgba: &[u8]) {
        let len = self.width as usize * self.height as usize * 4;
        let destination = unsafe { std::slice::from_raw_parts_mut(self.bits, len) };
        for (src, dst) in rgba.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
            // DIB32 is BGRA while tiny-skia is premultiplied RGBA.
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = src[3];
        }
    }
}

impl Drop for DibSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.hdc);
        }
    }
}

unsafe extern "system" fn overlay_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_center_matches_existing_position() {
        let point = calculate_overlay_origin(
            WorkArea {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            },
            Size {
                width: 600,
                height: 200,
            },
            OverlayPosition::BottomCenter,
            64,
        );
        assert_eq!(point, Point { x: 660, y: 776 });
    }

    #[test]
    fn top_right_respects_non_zero_monitor_origin() {
        let point = calculate_overlay_origin(
            WorkArea {
                left: 1920,
                top: -200,
                right: 4480,
                bottom: 1240,
            },
            Size {
                width: 500,
                height: 200,
            },
            OverlayPosition::TopRight,
            50,
        );
        assert_eq!(point, Point { x: 3930, y: -150 });
    }
}
