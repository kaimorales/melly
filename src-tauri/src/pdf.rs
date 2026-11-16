//! Text extraction and a page-addressed native PDF viewer.
#![allow(deprecated, unexpected_cfgs)]
use crate::Reply;
use melly_core::projects::{FilePassage, text_passages};

pub fn extract(name: &str, bytes: &[u8]) -> Reply<Vec<FilePassage>> {
    if crate::ocr::is_image(name) {
        return Ok(text_passages(&crate::ocr::text(bytes)?, None));
    }
    if name.to_ascii_lowercase().ends_with(".pdf") {
        return pdf_passages(bytes);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Choose a text, Markdown, or text-based PDF file")?;
    if text.contains('\0') || text.len() > 2 * 1024 * 1024 || text.trim().is_empty() {
        return Err(
            "Choose a nonempty text file up to 2 MiB, or a text-based PDF up to 20 MiB".into(),
        );
    }
    let passages = text_passages(text, None);
    if passages.len() > 2000 {
        return Err("This document contains too much text".into());
    }
    Ok(passages)
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use cocoa::{
        base::{id, nil},
        foundation::{NSAutoreleasePool, NSPoint, NSRect, NSSize},
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CStr;
    #[link(name = "PDFKit", kind = "framework")]
    unsafe extern "C" {}

    unsafe fn document(bytes: &[u8]) -> id {
        unsafe {
            let data: id =
                msg_send![class!(NSData), dataWithBytes:bytes.as_ptr() length:bytes.len()];
            let document: id = msg_send![class!(PDFDocument), alloc];
            msg_send![document, initWithData:data]
        }
    }
    pub fn passages(bytes: &[u8]) -> Reply<Vec<FilePassage>> {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let doc = document(bytes);
            let result = (|| {
                if doc == nil {
                    return Err("Could not read this PDF".into());
                }
                let locked: bool = msg_send![doc, isLocked];
                let count: usize = msg_send![doc, pageCount];
                if locked || count > 500 {
                    return Err("Choose an unlocked PDF with up to 500 pages".into());
                }
                let mut passages = Vec::new();
                let mut total = 0;
                let mut scanned = 0;
                for index in 0..count {
                    let page: id = msg_send![doc, pageAtIndex:index];
                    let string: id = msg_send![page, string];
                    let mut text = String::new();
                    if string != nil {
                        let ptr: *const std::ffi::c_char = msg_send![string, UTF8String];
                        if !ptr.is_null() {
                            text = CStr::from_ptr(ptr).to_string_lossy().into_owned();
                        }
                    }
                    if text.trim().is_empty() {
                        scanned += 1;
                        if scanned > 20 {
                            return Err("OCR supports up to 20 scanned pages per PDF. Split this file into smaller documents.".into());
                        }
                        let size = NSSize::new(1600., 2000.);
                        let thumbnail: id = msg_send![page, thumbnailOfSize:size forBox:0usize];
                        let data: id = msg_send![thumbnail, TIFFRepresentation];
                        if data != nil {
                            let length: usize = msg_send![data, length];
                            let ptr: *const u8 = msg_send![data, bytes];
                            if !ptr.is_null() && length <= 32 * 1024 * 1024 {
                                text =
                                    match crate::ocr::text(std::slice::from_raw_parts(ptr, length))
                                    {
                                        Ok(text) => text,
                                        Err(reason)
                                            if reason.starts_with("No readable text found") =>
                                        {
                                            String::new()
                                        }
                                        Err(reason) => {
                                            return Err(format!("Page {}: {reason}", index + 1));
                                        }
                                    };
                            }
                        }
                    }
                    total += text.len();
                    if total > 2 * 1024 * 1024 {
                        return Err("PDF text exceeds 2 MiB".into());
                    }
                    passages.extend(text_passages(&text, Some(index as u32 + 1)));
                    if passages.len() > 2000 {
                        return Err("PDF contains too much text".into());
                    }
                }
                if passages.is_empty() {
                    return Err("No readable text was found, including after on-device OCR.".into());
                }
                Ok(passages)
            })();
            if doc != nil {
                let _: () = msg_send![doc, release];
            }
            pool.drain();
            result
        }
    }
    thread_local! { static VIEWER: std::cell::Cell<id> = const { std::cell::Cell::new(nil) }; }
    pub fn show(bytes: &[u8], page: u32) -> Reply<()> {
        unsafe {
            let pool = NSAutoreleasePool::new(nil);
            let doc = document(bytes);
            let result = (|| {
                if doc == nil {
                    return Err("Could not open PDF".into());
                }
                let count: usize = msg_send![doc, pageCount];
                if page == 0 || page as usize > count {
                    return Err("The cited page is unavailable".into());
                }
                let window = VIEWER.with(|slot| {
                    if slot.get() != nil { return slot.get(); }
                    let window: id = msg_send![class!(NSWindow), alloc];
                    let rect = NSRect::new(NSPoint::new(0., 0.), NSSize::new(780., 780.));
                    let window: id = msg_send![window, initWithContentRect:rect styleMask:15u64 backing:2u64 defer:false];
                    let _: () = msg_send![window, setReleasedWhenClosed:false];
                    let view: id = msg_send![class!(PDFView), alloc];
                    let view: id = msg_send![view, initWithFrame:rect];
                    let _: () = msg_send![view, setAutoScales:true];
                    let _: () = msg_send![window, setContentView:view];
                    let _: () = msg_send![view, release];
                    let _: () = msg_send![window, center];
                    slot.set(window);
                    window
                });
                let view: id = msg_send![window, contentView];
                let _: () = msg_send![view, setDocument:doc];
                let target: id = msg_send![doc, pageAtIndex:(page - 1) as usize];
                let _: () = msg_send![view, goToPage:target];
                let _: () = msg_send![window, makeKeyAndOrderFront:nil];
                Ok(())
            })();
            if doc != nil {
                let _: () = msg_send![doc, release];
            }
            pool.drain();
            result
        }
    }
}
#[cfg(target_os = "macos")]
use native::passages as pdf_passages;
#[cfg(target_os = "macos")]
pub use native::show;
#[cfg(not(target_os = "macos"))]
fn pdf_passages(_: &[u8]) -> Reply<Vec<FilePassage>> {
    Err("PDF import requires macOS".into())
}
#[cfg(not(target_os = "macos"))]
pub fn show(_: &[u8], _: u32) -> Reply<()> {
    Err("PDF viewing requires macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_pdf_extraction_preserves_page_locations() {
        let stream = "BT /F1 18 Tf 72 700 Td (Synthetic transit result eighteen percent) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream",stream.len()),
        ];
        let mut pdf = "%PDF-1.4\n".to_string();
        let mut offsets = vec![0];
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        let passages = extract("synthetic.pdf", pdf.as_bytes()).unwrap();
        assert_eq!(passages[0].page, Some(1));
        assert!(passages[0].text.contains("eighteen"));
        assert!(extract("malformed.pdf", b"invalid").is_err());
        assert!(extract("binary.txt", b"\0binary").is_err());
    }
}
