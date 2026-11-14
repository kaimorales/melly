//! A native secure field positioned over the provider form. Secret bytes never
//! enter the webview, an IPC argument, a log, or frontend state.
#![allow(deprecated, unexpected_cfgs)]

use melly_core::credentials::SecretStore;
use serde::Deserialize;
use std::sync::Arc;
use tauri::WebviewWindow;

#[derive(Clone, Copy, Deserialize)]
pub struct FieldRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    visible: bool,
}

impl FieldRect {
    fn valid(self) -> bool {
        [self.left, self.top, self.width, self.height]
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 100_000.0)
            && self.width > 0.0
            && self.height > 0.0
    }
}

#[derive(Default)]
struct EntrySequence(u64);
impl EntrySequence {
    fn admit(&mut self, generation: u64) -> bool {
        if generation <= self.0 {
            return false;
        }
        self.0 = generation;
        true
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use cocoa::{
        base::{BOOL, NO, YES, id, nil},
        foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString},
    };
    use objc::{
        class,
        declare::ClassDecl,
        msg_send,
        runtime::{Class, Object, Sel},
        sel, sel_impl,
    };
    use std::{cell::RefCell, ffi::CStr, sync::OnceLock};

    struct Entry {
        generation: u64,
        provider: String,
        field: id,
        delegate: id,
    }
    impl Drop for Entry {
        fn drop(&mut self) {
            // Entries are created, used, and released on AppKit's main thread.
            unsafe {
                let editor: id = msg_send![self.field, currentEditor];
                if editor != nil {
                    let window: id = msg_send![self.field, window];
                    let _: BOOL = msg_send![window, makeFirstResponder: nil];
                }
                let empty = NSString::alloc(nil).init_str("");
                let _: () = msg_send![self.field, setStringValue: empty];
                let _: () = msg_send![self.field, setDelegate: nil];
                let _: () = msg_send![self.field, removeFromSuperview];
                let _: () = msg_send![empty, release];
                let _: () = msg_send![self.field, release];
                let _: () = msg_send![self.delegate, release];
            }
        }
    }
    thread_local! {
        static ENTRY: RefCell<Option<Entry>> = const { RefCell::new(None) };
        static SEQUENCE: RefCell<EntrySequence> = const { RefCell::new(EntrySequence(0)) };
    }

    extern "C" fn command(delegate: &Object, _: Sel, _: id, _: id, action: Sel) -> BOOL {
        let action = match action.name() {
            "insertTab:" => "next",
            "insertBacktab:" => "previous",
            "insertNewline:" => "save",
            "cancelOperation:" => "close",
            _ => return NO,
        };
        unsafe {
            let webview: id = *delegate.get_ivar("webview");
            let window: id = msg_send![webview, window];
            let _: BOOL = msg_send![window, makeFirstResponder: webview];
            let script = NSString::alloc(nil).init_str(&format!(
                "window.dispatchEvent(new CustomEvent('melly-native-key',{{detail:'{action}'}}))"
            ));
            let _: () = msg_send![webview, evaluateJavaScript: script completionHandler: nil];
            let _: () = msg_send![script, release];
        }
        YES
    }

    fn delegate_class() -> &'static Class {
        static CLASS: OnceLock<&'static Class> = OnceLock::new();
        CLASS.get_or_init(|| {
            let mut class = ClassDecl::new("MellyInlineSecureFieldDelegate", class!(NSObject))
                .expect("unique secure-field delegate class");
            class.add_ivar::<id>("webview");
            unsafe {
                class.add_method(
                    sel!(control:textView:doCommandBySelector:),
                    command as extern "C" fn(&Object, Sel, id, id, Sel) -> BOOL,
                );
            }
            class.register()
        })
    }

    async fn on_view<T: Send + 'static>(
        window: WebviewWindow,
        operation: impl FnOnce(id) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (send, receive) = tokio::sync::oneshot::channel();
        window
            .with_webview(move |view| {
                let pool = unsafe { NSAutoreleasePool::new(nil) };
                let result = operation(view.inner().cast());
                let _ = send.send(result);
                unsafe { pool.drain() };
            })
            .map_err(|_| "Could not open the API key field".to_owned())?;
        receive
            .await
            .map_err(|_| "The API key field is unavailable".to_owned())?
    }

    unsafe fn position(webview: id, field: id, rect: FieldRect) -> Result<(), String> {
        if !rect.valid() {
            return Err("Invalid API key field position".into());
        }
        unsafe {
            let parent: id = msg_send![webview, superview];
            if parent == nil {
                return Err("The API key field is unavailable".into());
            }
            let bounds: NSRect = msg_send![webview, bounds];
            let flipped: BOOL = msg_send![webview, isFlipped];
            let height = rect.height.min(24.0);
            let inset_x = (rect.width / 4.0).min(11.0);
            let top = rect.top + (rect.height - height) / 2.0 + 3.0;
            let y = if flipped == YES {
                top
            } else {
                bounds.size.height - top - height
            };
            let frame = NSRect::new(
                NSPoint::new(rect.left + inset_x, y),
                NSSize::new(rect.width - inset_x * 2.0, height),
            );
            let converted: NSRect = msg_send![webview, convertRect: frame toView: parent];
            if !rect.visible {
                let editor: id = msg_send![field, currentEditor];
                if editor != nil {
                    let window: id = msg_send![field, window];
                    let _: BOOL = msg_send![window, makeFirstResponder: nil];
                }
            }
            let _: () = msg_send![field, setFrame: converted];
            let _: () = msg_send![field, setHidden: if rect.visible { NO } else { YES }];
        }
        Ok(())
    }

    pub async fn open(
        window: WebviewWindow,
        generation: u64,
        provider: String,
        rect: FieldRect,
        theme: String,
        configured: bool,
    ) -> Result<(), String> {
        on_view(window, move |webview| {
            ENTRY.with(|entry| {
                let mut entry = entry.borrow_mut();
                if !SEQUENCE.with(|sequence| sequence.borrow_mut().admit(generation)) {
                    return Err("The API key field has changed".into());
                }
                unsafe {
                    let parent: id = msg_send![webview, superview];
                    if parent == nil || !rect.valid() { return Err("The API key field is unavailable".into()); }
                    let field: id = msg_send![class!(NSSecureTextField), alloc];
                    let field: id = msg_send![field, initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0))];
                    if field == nil { return Err("Could not create the API key field".into()); }
                    let placeholder = NSString::alloc(nil).init_str(if configured { "Enter a new key to replace" } else { "Paste API key" });
                    let label = NSString::alloc(nil).init_str("API key");
                    let appearance_name = NSString::alloc(nil).init_str(if theme == "dark" { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" });
                    let appearance: id = msg_send![class!(NSAppearance), appearanceNamed: appearance_name];
                    let font: id = msg_send![class!(NSFont), systemFontOfSize: 13.0f64];
                    let delegate: id = msg_send![delegate_class(), new];
                    (*delegate).set_ivar("webview", webview);
                    let _: () = msg_send![field, setPlaceholderString: placeholder];
                    let _: () = msg_send![field, setAccessibilityLabel: label];
                    let _: () = msg_send![field, setAppearance: appearance];
                    let _: () = msg_send![field, setFont: font];
                    let _: () = msg_send![field, setBezeled: NO];
                    let _: () = msg_send![field, setBordered: NO];
                    let _: () = msg_send![field, setDrawsBackground: NO];
                    let _: () = msg_send![field, setDelegate: delegate];
                    let _: () = msg_send![parent, addSubview: field positioned: 1isize relativeTo: webview];
                    let _: () = msg_send![placeholder, release];
                    let _: () = msg_send![label, release];
                    let _: () = msg_send![appearance_name, release];
                    let new_entry = Entry { generation, provider, field, delegate };
                    position(webview, field, rect)?;
                    *entry = Some(new_entry);
                }
                Ok(())
            })
        }).await
    }

    pub async fn update(
        window: WebviewWindow,
        generation: u64,
        rect: FieldRect,
        theme: String,
    ) -> Result<(), String> {
        on_view(window, move |webview| {
            ENTRY.with(|entry| {
                if let Some(entry) = entry
                    .borrow()
                    .as_ref()
                    .filter(|entry| entry.generation == generation)
                {
                    unsafe {
                        let name = NSString::alloc(nil).init_str(if theme == "dark" {
                            "NSAppearanceNameDarkAqua"
                        } else {
                            "NSAppearanceNameAqua"
                        });
                        let appearance: id = msg_send![class!(NSAppearance), appearanceNamed: name];
                        let _: () = msg_send![entry.field, setAppearance: appearance];
                        let _: () = msg_send![name, release];
                        position(webview, entry.field, rect)?;
                    }
                }
                Ok(())
            })
        })
        .await
    }

    pub async fn focus(window: WebviewWindow, generation: u64) -> Result<(), String> {
        on_view(window, move |_| {
            ENTRY.with(|entry| {
                let entry = entry.borrow();
                let field = entry
                    .as_ref()
                    .filter(|entry| entry.generation == generation)
                    .ok_or("The API key field is unavailable")?
                    .field;
                unsafe {
                    let window: id = msg_send![field, window];
                    let _: BOOL = msg_send![window, makeFirstResponder: field];
                }
                Ok(())
            })
        })
        .await
    }

    pub async fn save(
        window: WebviewWindow,
        generation: u64,
        provider: String,
        credential_id: String,
        vault: Arc<dyn SecretStore>,
    ) -> Result<bool, String> {
        on_view(window, move |_| {
            ENTRY.with(|entry| {
                let entry = entry.borrow();
                let field = entry
                    .as_ref()
                    .filter(|entry| entry.generation == generation && entry.provider == provider)
                    .ok_or("The API key field has changed")?
                    .field;
                unsafe {
                    let window: id = msg_send![field, window];
                    let _: BOOL = msg_send![window, makeFirstResponder: nil];
                    let value: id = msg_send![field, stringValue];
                    let bytes: *const std::os::raw::c_char = msg_send![value, UTF8String];
                    if bytes.is_null() {
                        return Err("Could not read the API key".into());
                    }
                    let value = CStr::from_ptr(bytes)
                        .to_str()
                        .map_err(|_| "Enter a valid API key")?
                        .trim();
                    if value.is_empty() {
                        return Ok(false);
                    }
                    if value.len() > 65536 || value.chars().any(char::is_control) {
                        return Err("Enter a valid API key".into());
                    }
                    vault
                        .set(&credential_id, value)
                        .map_err(|_| "Could not save the API key".to_owned())?;
                    let empty = NSString::alloc(nil).init_str("");
                    let _: () = msg_send![field, setStringValue: empty];
                    let _: () = msg_send![empty, release];
                }
                Ok(true)
            })
        })
        .await
    }

    pub async fn close(window: WebviewWindow, generation: u64) -> Result<(), String> {
        on_view(window, move |_| {
            ENTRY.with(|entry| {
                let mut entry = entry.borrow_mut();
                if entry
                    .as_ref()
                    .is_some_and(|entry| entry.generation == generation)
                {
                    *entry = None;
                }
                Ok(())
            })
        })
        .await
    }
}

#[cfg(target_os = "macos")]
pub use mac::{close, focus, open, save, update};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_field_mounts_cannot_reopen_after_a_newer_generation() {
        let mut sequence = EntrySequence::default();
        assert!(sequence.admit(10));
        assert!(sequence.admit(12));
        assert!(!sequence.admit(11));
        assert!(!sequence.admit(12));
        assert!(sequence.admit(13));
    }

    #[test]
    fn rejects_nonfinite_or_invalid_field_geometry() {
        let rect = FieldRect {
            left: 4.0,
            top: 5.0,
            width: 100.0,
            height: 36.0,
            visible: true,
        };
        assert!(rect.valid());
        assert!(!FieldRect { width: 0.0, ..rect }.valid());
        assert!(
            !FieldRect {
                height: -2.0,
                ..rect
            }
            .valid()
        );
        assert!(
            !FieldRect {
                left: f64::NAN,
                ..rect
            }
            .valid()
        );
        assert!(
            !FieldRect {
                top: f64::INFINITY,
                ..rect
            }
            .valid()
        );
        assert!(
            !FieldRect {
                width: 100_001.0,
                ..rect
            }
            .valid()
        );
    }
}
