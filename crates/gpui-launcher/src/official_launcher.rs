// SPDX-License-Identifier: MPL-2.0

//! Windows handoff to the installed official Minecraft Launcher.
//!
//! Calls the existing windows 0.61 activation API from the GPUI UI thread,
//! which has already initialized COM. No credentials, paths, arguments, or
//! account state are passed.

use windows::Win32::System::Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::{
    AO_NONE, ApplicationActivationManager, IApplicationActivationManager,
};
use windows::core::PCWSTR;

const MINECRAFT_LAUNCHER_AUMID: &str = "Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft";

pub fn activate_official_minecraft_launcher() -> Result<(), String> {
    let aumid = windows::core::HSTRING::from(MINECRAFT_LAUNCHER_AUMID);
    let manager: IApplicationActivationManager =
        unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER) }
            .map_err(|_| "Official Minecraft Launcher could not be opened.".to_owned())?;
    unsafe { manager.ActivateApplication(PCWSTR(aumid.as_ptr()), PCWSTR::null(), AO_NONE) }
        .map(|_| ())
        .map_err(|_| "Official Minecraft Launcher could not be opened.".to_owned())
}
