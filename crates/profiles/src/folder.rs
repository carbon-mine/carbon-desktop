use crate::storage;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn game_directory(root: &Path, id: Uuid) -> Result<PathBuf, String> {
    let (_lock, library) = storage::open(root)?;
    library.position(id)?;
    let instances = root.join("instances-v1");
    let instance = instances.join(id.to_string());
    let game = instance.join("game");
    for dir in [&instances, &instance, &game] {
        storage::private_directory(dir)?;
    }
    Ok(game)
}

/// Reveal only a named personal instance's private game directory in the system file manager.
pub fn open_game_folder(root: &Path, id: Uuid) -> Result<(), String> {
    let folder = game_directory(root, id)?;
    reveal(&folder)
}

#[cfg(windows)]
fn reveal(folder: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
    let mut buffer = [0u16; 32768];
    // This API returns the Windows installation directory for the operating system.
    let n =
        unsafe { GetSystemWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if n == 0 || n >= buffer.len() {
        return Err("Could not locate Windows Explorer.".into());
    }
    let explorer = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..n])).join("explorer.exe");
    std::process::Command::new(explorer)
        .arg(folder)
        .spawn()
        .map(|_| ())
        .map_err(|_| "Could not open the game folder in File Explorer.".into())
}

#[cfg(target_os = "linux")]
fn reveal(folder: &Path) -> Result<(), String> {
    use std::{process::Stdio, sync::mpsc, time::Duration};
    let mut child = std::process::Command::new("/usr/bin/xdg-open")
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "The desktop folder opener (xdg-open) is unavailable.")?;
    // Some desktops keep xdg-open alive for the lifetime of their file manager.
    let (finished, result) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = finished.send(child.wait());
    });
    match result.recv_timeout(Duration::from_millis(300)) {
        Ok(Ok(status)) if status.success() => Ok(()),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
        _ => Err("The desktop file manager could not open the game folder.".into()),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn reveal(_: &Path) -> Result<(), String> {
    Err("Opening a game folder is unavailable on this operating system.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_only_a_known_profiles_own_game_directory() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        assert!(game_directory(root, Uuid::new_v4()).is_err());
        assert!(!root.join("instances-v1").exists());
        crate::seed_review_library(root, true).unwrap();
        let profiles = crate::list(root).unwrap();
        let first = game_directory(root, profiles[0].id).unwrap();
        let second = game_directory(root, profiles[1].id).unwrap();
        assert_eq!(
            first,
            root.join("instances-v1")
                .join(profiles[0].id.to_string())
                .join("game")
        );
        assert_ne!(first, second);
        std::fs::write(first.join("options.txt"), b"personal").unwrap();
        assert!(!second.join("options.txt").exists());
        assert_eq!(game_directory(root, profiles[0].id).unwrap(), first);
    }
}
