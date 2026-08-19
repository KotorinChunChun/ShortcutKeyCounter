use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// Returns milliseconds since the last physical keyboard or mouse input without
/// exposing which key/button was pressed.
pub fn idle_milliseconds() -> Option<u64> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return None;
    }
    let now = unsafe { GetTickCount() };
    Some(now.wrapping_sub(info.dwTime) as u64)
}
