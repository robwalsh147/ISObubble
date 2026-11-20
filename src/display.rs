//! Monitors: name, resolution, current and maximum refresh rate, HDR support / on.

#[derive(Clone, Debug, Default)]
pub struct Monitor {
    /// 1 = main monitor, then the others in Windows' order.
    pub index: usize,
    /// Windows' device name, e.g. \\.\DISPLAY1 (used to remember the choice per game).
    pub device: String,
    pub name: String,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    pub max_hz: u32,
    pub bits: u32,
    pub hdr_supported: bool,
    pub hdr_on: bool,
}

impl Monitor {
    pub fn number(&self) -> String {
        self.index.to_string()
    }
    pub fn title(&self) -> String {
        format!(
            "Monitor {}{} — {}",
            self.number(),
            if self.primary { " (main)" } else { "" },
            if self.name.is_empty() { "Display".to_string() } else { self.name.clone() }
        )
    }
    pub fn summary(&self) -> String {
        let hz = if self.max_hz > self.hz { format!("{} Hz (up to {} Hz)", self.hz, self.max_hz) } else { format!("{} Hz", self.hz) };
        let hdr = match (self.hdr_supported, self.hdr_on) {
            (_, true) => "HDR on",
            (true, false) => "HDR supported, off",
            _ => "no HDR",
        };
        format!("{}×{} @ {hz} · {hdr}", self.width, self.height)
    }
}

/// All connected monitors, main monitor first then by number.
pub fn monitors() -> Vec<Monitor> {
    #[cfg(windows)]
    {
        let mut v = win::list();
        v.sort_by_key(|m| (!m.primary, m.device.trim_start_matches("\\\\.\\DISPLAY").parse::<u32>().unwrap_or(99)));
        for (i, m) in v.iter_mut().enumerate() {
            m.index = i + 1;
        }
        v
    }
    #[cfg(not(windows))]
    Vec::new()
}

pub fn by_device(device: &str) -> Option<Monitor> {
    monitors().into_iter().find(|m| m.device.eq_ignore_ascii_case(device))
}

#[cfg(windows)]
mod win {
    use super::Monitor;
    use windows_sys::Win32::Devices::Display::*;
    use windows_sys::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::*;

    fn wstr(b: &[u16]) -> String {
        let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..n])
    }

    unsafe extern "system" fn each(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
        let out = &mut *(lp as *mut Vec<Monitor>);
        let mut mi: MONITORINFOEXW = std::mem::zeroed();
        mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(h, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO) == 0 {
            return 1;
        }
        let r = mi.monitorInfo.rcMonitor;
        let device = wstr(&mi.szDevice);
        let mut m = Monitor {
            device: device.clone(),
            primary: mi.monitorInfo.dwFlags & 1 != 0,
            x: r.left,
            y: r.top,
            width: (r.right - r.left) as u32,
            height: (r.bottom - r.top) as u32,
            ..Default::default()
        };
        let name: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
        let mut dm: DEVMODEW = std::mem::zeroed();
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if EnumDisplaySettingsW(name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) != 0 {
            m.width = dm.dmPelsWidth;
            m.height = dm.dmPelsHeight;
            m.hz = dm.dmDisplayFrequency;
            m.bits = dm.dmBitsPerPel;
        }
        // Highest refresh rate available at this resolution.
        let mut i = 0u32;
        loop {
            let mut d: DEVMODEW = std::mem::zeroed();
            d.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            if EnumDisplaySettingsW(name.as_ptr(), i, &mut d) == 0 || i > 2000 {
                break;
            }
            if d.dmPelsWidth == m.width && d.dmPelsHeight == m.height {
                m.max_hz = m.max_hz.max(d.dmDisplayFrequency);
            }
            i += 1;
        }
        m.max_hz = m.max_hz.max(m.hz);
        out.push(m);
        1
    }

    pub fn list() -> Vec<Monitor> {
        let mut v: Vec<Monitor> = Vec::new();
        unsafe {
            EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut v as *mut Vec<Monitor> as LPARAM);
            // Friendly names and HDR state come from the display configuration.
            let (mut np, mut nm) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != 0 {
                return v;
            }
            let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![std::mem::zeroed(); np as usize];
            let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![std::mem::zeroed(); nm as usize];
            if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut np, paths.as_mut_ptr(), &mut nm, modes.as_mut_ptr(), std::ptr::null_mut()) != 0 {
                return v;
            }
            for p in paths.iter().take(np as usize) {
                let mut src: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
                src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
                src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
                src.header.adapterId = p.sourceInfo.adapterId;
                src.header.id = p.sourceInfo.id;
                if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                    continue;
                }
                let gdi = wstr(&src.viewGdiDeviceName);
                let Some(m) = v.iter_mut().find(|m| m.device.eq_ignore_ascii_case(&gdi)) else { continue };

                let mut tgt: DISPLAYCONFIG_TARGET_DEVICE_NAME = std::mem::zeroed();
                tgt.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
                tgt.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
                tgt.header.adapterId = p.targetInfo.adapterId;
                tgt.header.id = p.targetInfo.id;
                if DisplayConfigGetDeviceInfo(&mut tgt.header) == 0 {
                    m.name = wstr(&tgt.monitorFriendlyDeviceName);
                }

                let mut ac: DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO = std::mem::zeroed();
                ac.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO;
                ac.header.size = std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
                ac.header.adapterId = p.targetInfo.adapterId;
                ac.header.id = p.targetInfo.id;
                if DisplayConfigGetDeviceInfo(&mut ac.header) == 0 {
                    let bits = ac.Anonymous.value;
                    m.hdr_supported = bits & 1 != 0;
                    m.hdr_on = bits & 2 != 0;
                }
            }
        }
        v
    }
}

/// Does the game look like it supports HDR? Checks its settings files and its program.
pub fn game_hdr_support(program: &std::path::Path, files: &[std::path::PathBuf]) -> (Option<bool>, String) {
    for f in files {
        if let Ok(c) = crate::gamecfg::load(f) {
            if let Some(e) = c.entries.iter().find(|e| e.key.to_lowercase().contains("hdr")) {
                return (
                    Some(true),
                    format!("Yes — its settings have “{} = {}” in {}.", e.key, e.value, f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                );
            }
        }
    }
    let Ok(bytes) = std::fs::read(program) else {
        return (None, "Couldn't read the game's program.".into());
    };
    let region = &bytes[..bytes.len().min(96 * 1024 * 1024)];
    let hints = ["HDR10", "EnableHDR", "bHDR", "HDROutput", "HdrEnabled", "DXGI_COLOR_SPACE_RGB_FULL_G2084", "SetHDRMetaData", "scRGB"];
    if let Some(h) = hints.iter().find(|h| crate::pe::find(region, h.as_bytes()).is_some()) {
        return (Some(true), format!("Probably — the game's program mentions “{h}”. Turn it on in the game's own graphics menu."));
    }
    (Some(false), "No sign of HDR support (normal for games made before about 2017). Windows' Auto HDR can still brighten DirectX 11/12 games.".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_monitors() {
        let m = super::monitors();
        assert!(!m.is_empty());
        for x in &m {
            println!("{} | {}", x.title(), x.summary());
            assert!(x.width > 0 && x.hz > 0);
        }
    }
}
