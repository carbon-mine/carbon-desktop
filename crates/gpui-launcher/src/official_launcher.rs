// SPDX-License-Identifier: MPL-2.0

//! Windows handoff to the installed official Minecraft Launcher.
//!
//! Activation runs on a dedicated single-threaded apartment. The GPUI
//! background executor is already multi-threaded, and shell COM can dispatch
//! Windows messages during activation and reenter GPUI if called on the UI
//! thread. No account data, instance paths or launch arguments are passed.

use std::sync::mpsc;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize,
};
use windows::Win32::UI::Shell::{
    AO_NOERRORUI, ApplicationActivationManager, IApplicationActivationManager,
};
use windows::core::PCWSTR;

const MINECRAFT_LAUNCHER_AUMID: &str = "Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft";
const FAILED_TO_OPEN: &str = "Official Minecraft Launcher could not be opened.";

pub fn activate_official_minecraft_launcher() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("carbon-official-launcher".into())
        .spawn(move || {
            let _ = sender.send(activate_on_sta_thread());
        })
        .map_err(|_| FAILED_TO_OPEN.to_owned())?;
    let result = receiver.recv().map_err(|_| FAILED_TO_OPEN.to_owned());
    let _ = thread.join();
    result?
}

fn activate_on_sta_thread() -> Result<(), String> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|_| FAILED_TO_OPEN.to_owned())?;
    let result = activate().map_err(|_| FAILED_TO_OPEN.to_owned());
    unsafe { CoUninitialize() };
    result
}

fn activate() -> windows::core::Result<()> {
    let aumid = windows::core::HSTRING::from(MINECRAFT_LAUNCHER_AUMID);
    let manager: IApplicationActivationManager =
        unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_INPROC_SERVER) }?;
    unsafe { manager.ActivateApplication(PCWSTR(aumid.as_ptr()), PCWSTR::null(), AO_NOERRORUI) }
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::MINECRAFT_LAUNCHER_AUMID;

    #[test]
    fn aumid_is_the_installed_store_identity() {
        assert_eq!(
            MINECRAFT_LAUNCHER_AUMID,
            "Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft"
        );
    }
}
