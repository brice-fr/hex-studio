// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

pub mod a2l;
pub mod commands;
pub mod file_assoc;
pub mod file_operations;
pub mod hex_parser;
pub mod srec_parser;

use std::sync::Mutex;

// Emitter, Manager and RunEvent are only needed on macOS for the open-file handler.
#[cfg(target_os = "macos")]
use tauri::{Emitter, Manager, RunEvent};

/// Holds the path of the file to open at startup (CLI arg on Windows/Linux,
/// or macOS open-file Apple Event received before the webview is ready).
pub struct StartupFile(pub Mutex<Option<String>>);

/// Remove "Start Dictation…" and "Emoji & Symbols" — the two items macOS
/// silently appends to any NSMenu submenu named "Edit" — via NSUserDefaults
/// keys that AppKit checks at menu-build time.
#[cfg(target_os = "macos")]
fn suppress_edit_menu_extras() {
    use objc::runtime::{Object, YES};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let ud: *mut Object = msg_send![class!(NSUserDefaults), standardUserDefaults];
        for key in &[
            b"NSDisabledDictationMenuItem\0".as_ptr(),
            b"NSDisabledCharacterPaletteMenuItem\0".as_ptr(),
        ] {
            let ns_key: *mut Object = msg_send![
                class!(NSString),
                stringWithUTF8String: *key as *const i8
            ];
            let _: () = msg_send![ud, setBool: YES forKey: ns_key];
        }
    }
}

/// Register a lightweight ObjC NSMenuDelegate class ("HexEditMenuFilter") that
/// strips "Writing Tools" and "AutoFill" — injected by WKWebView via the
/// responder chain in macOS 15 — every time the Edit menu is about to open.
///
/// The NSUserDefaults approach used for Dictation/Emoji does not cover these
/// items because they are added dynamically per menu-open, not at build time.
#[cfg(target_os = "macos")]
fn register_edit_menu_filter() {
    use std::sync::Once;
    use objc::declare::ClassDecl;
    use objc::runtime::{Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    static REG: Once = Once::new();
    REG.call_once(|| unsafe {
        let Some(mut decl) = ClassDecl::new("HexEditMenuFilter", class!(NSObject)) else {
            return;
        };

        extern "C" fn menu_will_open(_this: &Object, _cmd: Sel, menu: *mut Object) {
            unsafe {
                // Pass 1: remove Writing Tools and AutoFill by title
                let n: isize = msg_send![menu, numberOfItems];
                let mut i = n - 1;
                while i >= 0 {
                    let item: *mut Object = msg_send![menu, itemAtIndex: i];
                    let title: *mut Object = msg_send![item, title];
                    let utf8: *const std::os::raw::c_char = msg_send![title, UTF8String];
                    if !utf8.is_null() {
                        let s = std::ffi::CStr::from_ptr(utf8).to_string_lossy();
                        if s.starts_with("Writing Tools")
                            || s == "AutoFill"
                            || s.starts_with("Autofill")
                        {
                            let _: () = msg_send![menu, removeItemAtIndex: i];
                        }
                    }
                    i -= 1;
                }

                // Pass 2: remove trailing separator items left by the removal above
                loop {
                    let count: isize = msg_send![menu, numberOfItems];
                    if count == 0 { break; }
                    let last: *mut Object = msg_send![menu, itemAtIndex: count - 1];
                    let is_sep: bool = msg_send![last, isSeparatorItem];
                    if is_sep {
                        let _: () = msg_send![menu, removeItemAtIndex: count - 1];
                    } else {
                        break;
                    }
                }

                // Pass 3: collapse consecutive separator items
                let n: isize = msg_send![menu, numberOfItems];
                let mut j = n - 1;
                while j > 0 {
                    let cur: *mut Object = msg_send![menu, itemAtIndex: j];
                    let prv: *mut Object = msg_send![menu, itemAtIndex: j - 1];
                    let cur_sep: bool = msg_send![cur, isSeparatorItem];
                    let prv_sep: bool = msg_send![prv, isSeparatorItem];
                    if cur_sep && prv_sep {
                        let _: () = msg_send![menu, removeItemAtIndex: j];
                    }
                    j -= 1;
                }
            }
        }

        decl.add_method(
            sel!(menuWillOpen:),
            menu_will_open as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.register();
    });
}

/// Find the "Edit" NSMenu in the app's main menu and attach the
/// HexEditMenuFilter delegate so it fires on every subsequent open.
/// Returns true when the delegate was successfully installed.
///
/// Must be called on the main thread after the JS onMount has built and
/// installed the app menu via `menu.setAsAppMenu()`.
#[cfg(target_os = "macos")]
fn attach_edit_menu_filter() -> bool {
    use objc::runtime::{Class, Object};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let main_menu: *mut Object = msg_send![app, mainMenu];
        if main_menu.is_null() { return false; }

        let count: isize = msg_send![main_menu, numberOfItems];
        for i in 0..count {
            let item: *mut Object = msg_send![main_menu, itemAtIndex: i];
            let title: *mut Object = msg_send![item, title];
            let utf8: *const std::os::raw::c_char = msg_send![title, UTF8String];
            if utf8.is_null() { continue; }
            if std::ffi::CStr::from_ptr(utf8).to_string_lossy() != "Edit" { continue; }

            let submenu: *mut Object = msg_send![item, submenu];
            if submenu.is_null() { break; }

            // Only install once
            let existing: *mut Object = msg_send![submenu, delegate];
            if !existing.is_null() { return true; }

            if let Some(cls) = Class::get("HexEditMenuFilter") {
                // `new` returns a +1 object; we intentionally "leak" the strong
                // reference so the delegate lives for the lifetime of the app.
                let delegate: *mut Object = msg_send![cls, new];
                let _: () = msg_send![submenu, setDelegate: delegate];
                return true;
            }
            break;
        }
        false
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // On Windows/Linux the OS passes the associated file as argv[1]
    let startup_path = std::env::args()
        .nth(1)
        .filter(|a| !a.starts_with('-') && std::path::Path::new(a).exists());

    tauri::Builder::default()
        .manage(StartupFile(Mutex::new(startup_path)))
        .manage(a2l::A2lState::new())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                suppress_edit_menu_extras();
                register_edit_menu_filter();

                // The JS onMount builds and installs the app menu asynchronously
                // after the webview loads.  We schedule the delegate attachment
                // 1.5 s later — well after onMount completes in practice.
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let _ = handle.run_on_main_thread(|| { attach_edit_menu_filter(); });
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::parse_file,
            commands::parse_intel_hex,
            commands::parse_srec,
            commands::detect_file_format,
            commands::save_file,
            commands::write_text_file,
            commands::save_binary,
            commands::get_file_associations,
            commands::apply_file_associations,
            commands::get_startup_file,
            commands::copy_plain_text,
            a2l::a2l_load,
            a2l::a2l_unload,
            a2l::a2l_list,
            a2l::a2l_detail,
            a2l::a2l_stats,
            a2l::a2l_encode_value,
            a2l::a2l_encode_text,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            // macOS sends RunEvent::Opened when the app is asked to open a file
            // via a file-association double-click (Apple Events / openFile:).
            // This variant only exists on macOS; other platforms use argv[1].
            #[cfg(target_os = "macos")]
            if let RunEvent::Opened { urls } = event {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        if let Some(path_str) = path.to_str() {
                            if let Some(state) = app_handle.try_state::<StartupFile>() {
                                if let Ok(mut guard) = state.0.lock() {
                                    *guard = Some(path_str.to_string());
                                }
                            }
                            let _ = app_handle.emit("open-file", path_str);
                        }
                    }
                }
            }
            // Suppress unused-variable warnings on non-macOS targets
            #[cfg(not(target_os = "macos"))]
            let _ = (app_handle, event);
        });
}
