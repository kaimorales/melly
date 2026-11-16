//! On-device Vision text recognition. No image or recognition request leaves the Mac.
#![allow(deprecated, unexpected_cfgs)]
use crate::Reply;
pub fn is_image(name: &str) -> bool {
    [".png", ".jpg", ".jpeg"]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(ext))
}
#[cfg(target_os = "macos")]
pub fn text(bytes: &[u8]) -> Reply<String> {
    use cocoa::{
        base::{id, nil},
        foundation::NSAutoreleasePool,
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CStr;
    #[link(name = "Vision", kind = "framework")]
    unsafe extern "C" {}
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let data: id = msg_send![class!(NSData),dataWithBytes:bytes.as_ptr() length:bytes.len()];
        let bitmap: id = msg_send![class!(NSBitmapImageRep),imageRepWithData:data];
        let result = (|| {
            if bitmap == nil {
                return Err("Choose a valid PNG or JPEG image".into());
            }
            let width: usize = msg_send![bitmap, pixelsWide];
            let height: usize = msg_send![bitmap, pixelsHigh];
            if width == 0 || height == 0 || width.saturating_mul(height) > 40_000_000 {
                return Err("Choose an image under 40 megapixels".into());
            }
            let request: id = msg_send![class!(VNRecognizeTextRequest), new];
            let _: () = msg_send![request,setRecognitionLevel:0usize];
            let _: () = msg_send![request,setUsesLanguageCorrection:true];
            let handler: id = msg_send![class!(VNImageRequestHandler), alloc];
            let options: id = msg_send![class!(NSDictionary), dictionary];
            let handler: id = msg_send![handler,initWithData:data options:options];
            let requests: id = msg_send![class!(NSArray),arrayWithObject:request];
            let mut failure: id = nil;
            let ok: bool = msg_send![handler,performRequests:requests error:&mut failure];
            let mut output = String::new();
            if ok {
                let results: id = msg_send![request, results];
                let count: usize = msg_send![results, count];
                for i in 0..count.min(10000) {
                    let observation: id = msg_send![results,objectAtIndex:i];
                    let candidates: id = msg_send![observation,topCandidates:1usize];
                    let candidate: id = msg_send![candidates, firstObject];
                    if candidate == nil {
                        continue;
                    }
                    let string: id = msg_send![candidate, string];
                    let ptr: *const std::ffi::c_char = msg_send![string, UTF8String];
                    if !ptr.is_null() {
                        output.push_str(&CStr::from_ptr(ptr).to_string_lossy());
                        output.push('\n');
                    }
                    if output.len() > 2 * 1024 * 1024 {
                        break;
                    }
                }
            }
            let _: () = msg_send![handler, release];
            let _: () = msg_send![request, release];
            if !ok {
                let code: isize = if failure != nil {
                    msg_send![failure, code]
                } else {
                    0
                };
                return Err(format!(
                    "On-device text recognition is unavailable (Vision {code}). Try a text-based PDF or text file."
                ));
            }
            if output.trim().is_empty() {
                return Err("No readable text found. Image attachments currently share recognized text, not the image itself.".into());
            }
            if output.len() > 2 * 1024 * 1024 {
                return Err("Recognized text exceeds 2 MiB".into());
            }
            Ok(output)
        })();
        pool.drain();
        result
    }
}
#[cfg(not(target_os = "macos"))]
pub fn text(_: &[u8]) -> Reply<String> {
    Err("Image text recognition requires macOS".into())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    #[ignore = "Requires a native macOS session with Vision services; restricted runners return nilError"]
    fn recognizes_synthetic_image_and_scanned_pdf_on_device() {
        let image = include_bytes!("../../tests/fixtures/ocr/receipt.png");
        let text = super::text(image).unwrap();
        assert!(
            text.contains("4343"),
            "OCR should read the synthetic reference code"
        );
        let pdf = include_bytes!("../../tests/fixtures/ocr/scanned.pdf");
        let passages = crate::pdf::extract("scanned.pdf", pdf).unwrap();
        assert!(
            passages
                .iter()
                .any(|p| p.page == Some(1) && p.text.contains("4343"))
        );
        assert!(super::text(b"not an image").is_err());
    }
}
