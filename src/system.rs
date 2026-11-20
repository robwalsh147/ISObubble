//! Talks to Windows: virtual disc drive (mount/eject), running setup programs,
//! finding newly installed programs, and installing ISObubble itself.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_NAME: &str = "ISObubble";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn hide_window(c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = c;
}

/// Quotes a string for a PowerShell single-quoted literal.
pub fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn ps(script: &str) -> Result<String, String> {
    let mut c = Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
    hide_window(&mut c);
    let o = c.output().map_err(|e| format!("Couldn't start PowerShell: {e}"))?;
    let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if o.status.success() {
        Ok(out)
    } else {
        let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
        let first = err.lines().next().unwrap_or("").to_string();
        Err(if first.is_empty() { out } else { first })
    }
}

/// Like `ps`, but hands each output line to `on_line` as it arrives (for progress).
pub fn ps_stream(script: &str, on_line: &mut dyn FnMut(&str)) -> Result<(), String> {
    use std::io::BufRead;
    let mut c = Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);
    c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    hide_window(&mut c);
    let mut child = c.spawn().map_err(|e| format!("Couldn't start PowerShell: {e}"))?;
    if let Some(out) = child.stdout.take() {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            on_line(&line);
        }
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut err);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(err.lines().find(|l| !l.trim().is_empty()).unwrap_or("PowerShell failed").to_string())
    }
}

/// "Inserts" the disc image into a virtual DVD drive and returns its drive letter.
pub fn mount_iso(path: &Path) -> Result<char, String> {
    let p = q(&abs(path));
    let script = format!(
        "$i = Get-DiskImage -ImagePath {p}; if (-not $i.Attached) {{ Mount-DiskImage -ImagePath {p} -Access ReadOnly -StorageType ISO | Out-Null }}; \
         for ($n=0; $n -lt 20; $n++) {{ $l = (Get-DiskImage -ImagePath {p} | Get-Volume).DriveLetter; if ($l) {{ $l; break }}; Start-Sleep -Milliseconds 300 }}"
    );
    let out = ps(&script)?;
    out.chars()
        .find(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| "Windows mounted the disc but didn't give it a drive letter.".to_string())
}

pub fn is_mounted(path: &Path) -> bool {
    ps(&format!("(Get-DiskImage -ImagePath {}).Attached", q(&abs(path))))
        .map(|s| s.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// "Ejects" the disc image from the virtual drive.
pub fn dismount_iso(path: &Path) -> Result<(), String> {
    ps(&format!("Dismount-DiskImage -ImagePath {} | Out-Null", q(&abs(path)))).map(|_| ())
}

/// Starts a program the way Explorer would (asks for admin rights if the program needs them)
/// and waits for it to close. Returns its exit code if known.
pub fn run_and_wait(program: &Path, workdir: &Path) -> Result<Option<i32>, String> {
    let is_msi = program.extension().map(|e| e.eq_ignore_ascii_case("msi")).unwrap_or(false);
    let script = if is_msi {
        format!(
            "$p = Start-Process -FilePath 'msiexec.exe' -ArgumentList ('/i \"' + {} + '\"') -WorkingDirectory {} -PassThru -Wait; $p.ExitCode",
            q(&program.to_string_lossy()),
            q(&workdir.to_string_lossy())
        )
    } else {
        format!(
            "$p = Start-Process -FilePath {} -WorkingDirectory {} -PassThru -Wait; $p.ExitCode",
            q(&program.to_string_lossy()),
            q(&workdir.to_string_lossy())
        )
    };
    let out = ps(&script)?;
    Ok(out.lines().last().and_then(|l| l.trim().parse().ok()))
}

/// Starts a program and returns immediately.
pub fn launch(program: &Path) -> Result<(), String> {
    let wd = program.parent().unwrap_or(Path::new("."));
    ps(&format!(
        "Start-Process -FilePath {} -WorkingDirectory {}",
        q(&program.to_string_lossy()),
        q(&wd.to_string_lossy())
    ))
    .map(|_| ())
}

/// Starts a program with options, a Windows compatibility mode (e.g. "WINXPSP3 RUNASADMIN",
/// applied just for this launch) and an optional working folder.
pub fn launch_full(program: &Path, args: &str, compat: &str, workdir: Option<&Path>) -> Result<(), String> {
    let wd = workdir.or(program.parent()).unwrap_or(Path::new("."));
    let mut script = String::new();
    let layers: Vec<&str> = compat.split_whitespace().filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric())).collect();
    let admin = layers.contains(&"RUNASADMIN");
    if !layers.is_empty() {
        script.push_str(&format!("$env:__COMPAT_LAYER = '{}'; ", layers.join(" ")));
    }
    script.push_str(&format!("Start-Process -FilePath {} -WorkingDirectory {}", q(&program.to_string_lossy()), q(&wd.to_string_lossy())));
    if !args.trim().is_empty() {
        script.push_str(&format!(" -ArgumentList {}", q(args.trim())));
    }
    if admin {
        script.push_str(" -Verb RunAs");
    }
    ps(&script).map(|_| ())
}

pub fn open_url(url: &str) {
    if url.starts_with("https://") || url.starts_with("http://") || url.starts_with("ms-settings:") {
        let mut c = Command::new("explorer.exe");
        c.arg(url);
        let _ = c.spawn();
    }
}

/// Start ISObubble (quietly, in the tray) when Windows starts — a shortcut in the Startup folder.
pub fn set_start_with_windows(on: bool) -> Result<(), String> {
    let exe = if is_installed() { installed_exe() } else { std::env::current_exe().map_err(|e| e.to_string())? };
    let script = if on {
        format!(
            "$s = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path ([Environment]::GetFolderPath('Startup')) '{APP_NAME}.lnk')); $s.TargetPath = {}; $s.Arguments = '--tray'; $s.WorkingDirectory = {}; $s.Save()",
            q(&exe.to_string_lossy()),
            q(&exe.parent().unwrap_or(Path::new(".")).to_string_lossy())
        )
    } else {
        format!("Remove-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath('Startup')) '{APP_NAME}.lnk') -ErrorAction SilentlyContinue")
    };
    ps(&script).map(|_| ())
}

pub fn starts_with_windows() -> bool {
    let base = std::env::var("APPDATA").unwrap_or_default();
    Path::new(&base).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup").join(format!("{APP_NAME}.lnk")).exists()
}

/// Starts a program with extra command-line options (e.g. "-windowed -noborder").
#[allow(dead_code)]
pub fn launch_with(program: &Path, args: &str) -> Result<(), String> {
    if args.trim().is_empty() {
        return launch(program);
    }
    let wd = program.parent().unwrap_or(Path::new("."));
    ps(&format!(
        "Start-Process -FilePath {} -WorkingDirectory {} -ArgumentList {}",
        q(&program.to_string_lossy()),
        q(&wd.to_string_lossy()),
        q(args.trim())
    ))
    .map(|_| ())
}

pub fn open_folder(path: &Path) {
    let mut c = Command::new("explorer.exe");
    c.arg(path);
    let _ = c.spawn();
}

/// Start-menu / desktop shortcuts created after `since` (unix seconds) that point at programs.
pub fn find_new_shortcuts(since: u64) -> Vec<(String, String)> {
    let script = format!(
        "$since = [DateTimeOffset]::FromUnixTimeSeconds({since}).LocalDateTime.AddMinutes(-2); $sh = New-Object -ComObject WScript.Shell; \
         $dirs = @([Environment]::GetFolderPath('CommonStartMenu'), [Environment]::GetFolderPath('StartMenu'), [Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('CommonDesktopDirectory')); \
         foreach ($d in $dirs) {{ if ($d) {{ Get-ChildItem -LiteralPath $d -Recurse -Filter *.lnk -ErrorAction SilentlyContinue | Where-Object {{ $_.CreationTime -ge $since -or $_.LastWriteTime -ge $since }} | ForEach-Object {{ $t = $sh.CreateShortcut($_.FullName).TargetPath; if ($t -like '*.exe') {{ $_.BaseName + '|' + $t }} }} }} }}"
    );
    let mut out: Vec<(String, String)> = ps(&script)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('|'))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .filter(|(n, t)| {
            let l = format!("{n} {t}").to_lowercase();
            !l.contains("unins") && !l.contains("readme") && !l.contains("isobubble")
        })
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.1.eq_ignore_ascii_case(&b.1));
    out
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------- Installing ISObubble itself ----------

pub fn install_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    Path::new(&base).join("Programs").join(APP_NAME)
}

pub fn installed_exe() -> PathBuf {
    install_dir().join(format!("{APP_NAME}.exe"))
}

pub fn is_installed() -> bool {
    installed_exe().exists()
}

pub fn running_from_install() -> bool {
    match (std::env::current_exe(), std::fs::canonicalize(installed_exe())) {
        (Ok(a), Ok(b)) => std::fs::canonicalize(a).map(|a| a == b).unwrap_or(false),
        _ => false,
    }
}

pub fn installed_icon() -> PathBuf {
    install_dir().join(format!("{APP_NAME}.ico"))
}

fn shortcut_script(lnk_folder_expr: &str, exe: &Path) -> String {
    let icon = installed_icon();
    let icon = if icon.exists() { icon } else { exe.to_path_buf() };
    format!(
        "$f = {lnk_folder_expr}; $s = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $f '{APP_NAME}.lnk')); $s.TargetPath = {}; $s.WorkingDirectory = {}; $s.Description = 'Install discs and launch your programs'; $s.IconLocation = {}; $s.Save();",
        q(&exe.to_string_lossy()),
        q(&exe.parent().unwrap_or(Path::new(".")).to_string_lossy()),
        q(&format!("{},0", icon.to_string_lossy()))
    )
}

/// Copies this program into the user's Programs folder, adds Start-menu (and optionally
/// desktop) shortcuts and registers it in "Apps & features" so it can be uninstalled.
pub fn self_install(desktop: bool) -> Result<String, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let exe = installed_exe();
    if !running_from_install() {
        std::fs::copy(&me, &exe).map_err(|e| format!("Couldn't copy the app: {e}"))?;
    }
    // The app's icon, used by the desktop / Start-menu shortcuts and Settings › Apps.
    std::fs::write(installed_icon(), crate::theme::icon_ico_bytes()).map_err(|e| format!("Couldn't write the icon: {e}"))?;
    let mut script = shortcut_script("[Environment]::GetFolderPath('Programs')", &exe);
    if desktop {
        script.push_str(&shortcut_script("[Environment]::GetFolderPath('Desktop')", &exe));
    }
    let key = format!("HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}");
    let size_kb = std::fs::metadata(&exe).map(|m| m.len() / 1024).unwrap_or(0);
    script.push_str(&format!(
        "New-Item -Path {k} -Force | Out-Null; \
         Set-ItemProperty -Path {k} -Name DisplayName -Value '{APP_NAME}'; \
         Set-ItemProperty -Path {k} -Name DisplayVersion -Value '{APP_VERSION}'; \
         Set-ItemProperty -Path {k} -Name Publisher -Value '{APP_NAME}'; \
         Set-ItemProperty -Path {k} -Name InstallLocation -Value {dir}; \
         Set-ItemProperty -Path {k} -Name DisplayIcon -Value {ico}; \
         Set-ItemProperty -Path {k} -Name UninstallString -Value {un}; \
         Set-ItemProperty -Path {k} -Name EstimatedSize -Value {size_kb} -Type DWord; \
         Set-ItemProperty -Path {k} -Name NoModify -Value 1 -Type DWord; \
         Set-ItemProperty -Path {k} -Name NoRepair -Value 1 -Type DWord;",
        k = q(&key),
        dir = q(&dir.to_string_lossy()),
        ico = q(&installed_icon().to_string_lossy()),
        un = q(&format!("\"{}\" --uninstall", exe.to_string_lossy())),
    ));
    ps(&script)?;
    Ok(format!(
        "{APP_NAME} is installed. Find it in the Start menu{}, or in Settings › Apps to uninstall.",
        if desktop { " and on your desktop" } else { "" }
    ))
}

/// Removes shortcuts and the "Apps & features" entry, then deletes the install folder
/// once this program has closed.
pub fn self_uninstall() -> Result<(), String> {
    let key = format!("HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_NAME}");
    let script = format!(
        "Remove-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath('Programs')) '{APP_NAME}.lnk') -ErrorAction SilentlyContinue; \
         Remove-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath('Desktop')) '{APP_NAME}.lnk') -ErrorAction SilentlyContinue; \
         Remove-Item -Path {} -Recurse -ErrorAction SilentlyContinue; 'ok'",
        q(&key)
    );
    ps(&script)?;
    let dir = install_dir();
    let mut c = Command::new("cmd.exe");
    c.arg("/C")
        .arg(format!("ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.to_string_lossy()));
    hide_window(&mut c);
    c.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn self_uninstall_interactive() {
    let yes = rfd::MessageDialog::new()
        .set_title("Uninstall ISObubble")
        .set_description("Remove ISObubble from this computer? Programs you installed from discs stay installed, and your library list is kept.")
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    if yes == rfd::MessageDialogResult::Yes {
        let msg = match self_uninstall() {
            Ok(()) => "ISObubble has been removed.".to_string(),
            Err(e) => format!("Uninstall didn't finish: {e}"),
        };
        rfd::MessageDialog::new().set_title("ISObubble").set_description(msg).show();
    }
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    Path::new(&base).join(APP_NAME)
}

pub fn default_install_root() -> PathBuf {
    let base = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
    Path::new(&base).join("ISObubble Installs")
}

pub fn safe_folder_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let t = t.trim().trim_end_matches('.').to_string();
    if t.is_empty() { "Disc".into() } else { t }
}

/// Full path (Windows' disc-image commands ignore relative paths).
fn abs(p: &Path) -> String {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_string()
}
