// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

use serde::{Deserialize, Serialize};

/// One row in the file-association status table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssocEntry {
    pub ext: String,           // "hex"
    pub group: String,         // "Intel HEX"
    pub description: String,   // "Intel HEX firmware file"
    pub associated: bool,      // current OS status
    pub can_deassociate: bool, // false on macOS
}

/// All extensions this app can manage.
pub const EXTENSIONS: &[(&str, &str, &str)] = &[
    // (extension, group, description)
    ("hex",  "Intel HEX",         "Intel HEX firmware file"),
    ("ihex", "Intel HEX",         "Intel HEX firmware file (alternate)"),
    ("srec", "Motorola S-record", "Motorola S-record"),
    ("s19",  "Motorola S-record", "S-record 16-bit address"),
    ("s28",  "Motorola S-record", "S-record 24-bit address"),
    ("s37",  "Motorola S-record", "S-record 32-bit address"),
    ("mot",  "Motorola S-record", "Motorola S-record (alternate)"),
    ("bin",  "Binary",            "Raw binary firmware file"),
];

/// Return current association status for all managed extensions.
pub fn get_associations() -> Vec<AssocEntry> {
    platform::get_associations()
}

/// Apply a batch of association changes.
/// `changes` is a list of (extension, associate:bool).
pub fn apply_associations(changes: &[(String, bool)]) -> Result<(), String> {
    platform::apply_associations(changes)
}

// ── Platform dispatch ────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::path::PathBuf;
    use winreg::enums::*;
    use winreg::RegKey;

    const PROG_ID: &str = "HexEditorFile";

    fn exe_path() -> Option<PathBuf> {
        std::env::current_exe().ok()
    }

    fn ensure_prog_id() -> Result<(), String> {
        let exe = exe_path().ok_or("Cannot determine exe path")?;
        let exe_str = exe.to_string_lossy();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let (prog_key, _) = hkcu
            .create_subkey(format!("Software\\Classes\\{}", PROG_ID))
            .map_err(|e| e.to_string())?;
        prog_key.set_value("", &"Firmware File").map_err(|e| e.to_string())?;

        let (cmd_key, _) = hkcu
            .create_subkey(format!("Software\\Classes\\{}\\shell\\open\\command", PROG_ID))
            .map_err(|e| e.to_string())?;
        cmd_key.set_value("", &format!("\"{}\" \"%1\"", exe_str))
            .map_err(|e| e.to_string())?;

        let (icon_key, _) = hkcu
            .create_subkey(format!("Software\\Classes\\{}\\DefaultIcon", PROG_ID))
            .map_err(|e| e.to_string())?;
        icon_key.set_value("", &format!("\"{}\",0", exe_str))
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn is_associated(ext: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey(format!("Software\\Classes\\.{}", ext)) {
            if let Ok(val) = key.get_value::<String, _>("") {
                return val == PROG_ID;
            }
        }
        let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
        if let Ok(key) = hkcr.open_subkey(format!(".{}", ext)) {
            if let Ok(val) = key.get_value::<String, _>("") {
                return val == PROG_ID;
            }
        }
        false
    }

    fn associate(ext: &str) -> Result<(), String> {
        ensure_prog_id()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(format!("Software\\Classes\\.{}", ext))
            .map_err(|e| e.to_string())?;
        key.set_value("", &PROG_ID).map_err(|e| e.to_string())?;
        notify_shell();
        Ok(())
    }

    fn deassociate(ext: &str) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(format!("Software\\Classes\\.{}", ext));
        notify_shell();
        Ok(())
    }

    fn notify_shell() {
        use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }

    pub fn get_associations() -> Vec<AssocEntry> {
        EXTENSIONS.iter().map(|(ext, group, desc)| AssocEntry {
            ext: ext.to_string(),
            group: group.to_string(),
            description: desc.to_string(),
            associated: is_associated(ext),
            can_deassociate: true,
        }).collect()
    }

    pub fn apply_associations(changes: &[(String, bool)]) -> Result<(), String> {
        for (ext, assoc) in changes {
            if *assoc { associate(ext)?; } else { deassociate(ext)?; }
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};

    const BUNDLE_ID: &str = "com.brice-dev.hex-studio";

    extern "C" {
        fn UTTypeCreatePreferredIdentifierForTag(
            tag_class: CFStringRef,
            tag: CFStringRef,
            conforming_to: CFStringRef,
        ) -> CFStringRef;

        fn LSCopyDefaultRoleHandlerForContentType(
            content_type: CFStringRef,
            role: u32,
        ) -> CFStringRef;

        fn LSSetDefaultRoleHandlerForContentType(
            content_type: CFStringRef,
            role: u32,
            handler_bundle_id: CFStringRef,
        ) -> i32;
    }

    const LS_ROLES_ALL: u32 = 0xF;

    fn uti_for_ext(ext: &str) -> Option<String> {
        let tag_class = CFString::new("public.filename-extension");
        let tag = CFString::new(ext);

        unsafe {
            let result = UTTypeCreatePreferredIdentifierForTag(
                tag_class.as_concrete_TypeRef(),
                tag.as_concrete_TypeRef(),
                std::ptr::null(),
            );
            if result.is_null() { return None; }
            let s = CFString::wrap_under_create_rule(result);
            Some(s.to_string())
        }
    }

    fn is_associated(ext: &str) -> bool {
        let Some(uti) = uti_for_ext(ext) else { return false; };
        unsafe {
            let uti_cf = CFString::new(&uti);
            let handler_ref = LSCopyDefaultRoleHandlerForContentType(
                uti_cf.as_concrete_TypeRef(),
                LS_ROLES_ALL,
            );
            if handler_ref.is_null() { return false; }
            let handler = CFString::wrap_under_create_rule(handler_ref);
            handler.to_string().to_lowercase() == BUNDLE_ID.to_lowercase()
        }
    }

    fn associate(ext: &str) -> Result<(), String> {
        let uti = uti_for_ext(ext).ok_or_else(|| format!("Unknown UTI for .{}", ext))?;
        unsafe {
            let uti_cf = CFString::new(&uti);
            let bid_cf = CFString::new(BUNDLE_ID);
            let status = LSSetDefaultRoleHandlerForContentType(
                uti_cf.as_concrete_TypeRef(),
                LS_ROLES_ALL,
                bid_cf.as_concrete_TypeRef(),
            );
            if status == 0 { Ok(()) }
            else { Err(format!("LSSetDefaultRoleHandlerForContentType returned {}", status)) }
        }
    }

    pub fn get_associations() -> Vec<AssocEntry> {
        EXTENSIONS.iter().map(|(ext, group, desc)| AssocEntry {
            ext: ext.to_string(),
            group: group.to_string(),
            description: desc.to_string(),
            associated: is_associated(ext),
            can_deassociate: false,
        }).collect()
    }

    pub fn apply_associations(changes: &[(String, bool)]) -> Result<(), String> {
        let mut errors = vec![];
        for (ext, assoc) in changes {
            if *assoc {
                if let Err(e) = associate(ext) { errors.push(format!(".{}: {}", ext, e)); }
            }
        }
        if errors.is_empty() { Ok(()) }
        else { Err(errors.join("; ")) }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::process::Command;

    fn mime_for_ext(ext: &str) -> &'static str {
        match ext {
            "hex" | "ihex"                            => "text/x-ihex",
            "srec" | "s19" | "s28" | "s37" | "mot"  => "text/x-srecord",
            "bin"                                     => "application/x-firmware-binary",
            _                                         => "application/octet-stream",
        }
    }

    const DESKTOP_FILE: &str = "hex-studio.desktop";

    fn is_associated(ext: &str) -> bool {
        let mime = mime_for_ext(ext);
        let out = Command::new("xdg-mime")
            .args(["query", "default", mime])
            .output();
        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout).to_lowercase();
                s.contains("hex-studio")
            }
            Err(_) => false,
        }
    }

    fn associate(ext: &str) -> Result<(), String> {
        let mime = mime_for_ext(ext);
        let status = Command::new("xdg-mime")
            .args(["default", DESKTOP_FILE, mime])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() { Ok(()) }
        else { Err(format!("xdg-mime failed for .{}", ext)) }
    }

    fn deassociate(ext: &str) -> Result<(), String> {
        let mime = mime_for_ext(ext);
        let home = std::env::var("HOME").unwrap_or_default();
        let path = format!("{}/.config/mimeapps.list", home);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let filtered: String = content.lines()
                .filter(|line| {
                    let trimmed = line.trim_start();
                    !(trimmed.starts_with(mime) && trimmed.contains(DESKTOP_FILE))
                })
                .map(|l| format!("{}\n", l))
                .collect();
            std::fs::write(&path, filtered).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn get_associations() -> Vec<AssocEntry> {
        EXTENSIONS.iter().map(|(ext, group, desc)| AssocEntry {
            ext: ext.to_string(),
            group: group.to_string(),
            description: desc.to_string(),
            associated: is_associated(ext),
            can_deassociate: true,
        }).collect()
    }

    pub fn apply_associations(changes: &[(String, bool)]) -> Result<(), String> {
        let mut errors = vec![];
        for (ext, assoc) in changes {
            let result = if *assoc { associate(ext) } else { deassociate(ext) };
            if let Err(e) = result { errors.push(e); }
        }
        if errors.is_empty() { Ok(()) }
        else { Err(errors.join("; ")) }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
mod platform {
    use super::*;
    pub fn get_associations() -> Vec<AssocEntry> {
        EXTENSIONS.iter().map(|(ext, group, desc)| AssocEntry {
            ext: ext.to_string(), group: group.to_string(),
            description: desc.to_string(), associated: false, can_deassociate: false,
        }).collect()
    }
    pub fn apply_associations(_: &[(String, bool)]) -> Result<(), String> {
        Err("File association management is not supported on this platform.".into())
    }
}
