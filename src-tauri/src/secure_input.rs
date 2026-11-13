//! Provider secrets are entered in AppKit, never in the webview or an IPC reply.
#![allow(deprecated, unexpected_cfgs)]

pub async fn prompt_secret(app: tauri::AppHandle, label: String) -> Result<Option<String>, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = send.send(native_prompt(&label));
    })
    .map_err(|_| "The secure credential dialog could not be opened".to_owned())?;
    receive
        .await
        .map_err(|_| "The secure credential dialog was closed".to_owned())?
}

#[cfg(target_os = "macos")]
fn native_prompt(label: &str) -> Result<Option<String>, String> {
    use cocoa::{
        base::{id, nil},
        foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString},
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CStr;

    // run_on_main_thread above is the only call site. AppKit owns the modal and
    // secure text field; all copied bytes stay in Rust and go directly to Keychain.
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let alert: id = msg_send![class!(NSAlert), new];
        let field: id = msg_send![class!(NSSecureTextField), alloc];
        let field: id = msg_send![field, initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(360.0, 28.0))];
        if alert == nil || field == nil {
            if field != nil {
                let _: () = msg_send![field, release];
            }
            if alert != nil {
                let _: () = msg_send![alert, release];
            }
            pool.drain();
            return Err("The secure credential dialog could not be created".to_owned());
        }
        let title = format!(
            "Add {} key",
            label
                .chars()
                .filter(|c| !c.is_control())
                .take(100)
                .collect::<String>()
        );
        let title = NSString::alloc(nil).init_str(&title);
        let information = NSString::alloc(nil).init_str(
            "Your key is stored in macOS Keychain. It is sent only to the selected provider.",
        );
        let save = NSString::alloc(nil).init_str("Save key");
        let cancel = NSString::alloc(nil).init_str("Cancel");
        let empty = NSString::alloc(nil).init_str("");
        let _: () = msg_send![alert, setMessageText: title];
        let _: () = msg_send![alert, setInformativeText: information];
        let _: id = msg_send![alert, addButtonWithTitle: save];
        let _: id = msg_send![alert, addButtonWithTitle: cancel];
        let _: () = msg_send![alert, setAccessoryView: field];
        let _: () = msg_send![field, setPlaceholderString: empty];
        let window: id = msg_send![alert, window];
        let _: () = msg_send![window, setInitialFirstResponder: field];
        let response: isize = msg_send![alert, runModal];
        let result = if response == 1000 {
            let value: id = msg_send![field, stringValue];
            let bytes: *const std::os::raw::c_char = msg_send![value, UTF8String];
            if bytes.is_null() {
                Err("The key could not be read from the secure dialog".to_owned())
            } else {
                match CStr::from_ptr(bytes).to_str() {
                    Ok(value) if value.trim().is_empty() => Ok(None),
                    Ok(value) if value.len() <= 65536 && !value.chars().any(char::is_control) => {
                        Ok(Some(value.trim().to_owned()))
                    }
                    _ => Err("Enter a valid provider key".to_owned()),
                }
            }
        } else {
            Ok(None)
        };
        let _: () = msg_send![field, setStringValue: empty];
        for object in [title, information, save, cancel, empty, field, alert] {
            let _: () = msg_send![object, release];
        }
        pool.drain();
        result
    }
}

#[cfg(not(target_os = "macos"))]
fn native_prompt(_label: &str) -> Result<Option<String>, String> {
    Err("Secure credential entry requires macOS".to_owned())
}
