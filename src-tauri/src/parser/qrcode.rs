use crate::model::ProxyNode;
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::node_to_share_link;
use image::{DynamicImage, GrayImage};
use qrcode::render::svg;
use qrcode::QrCode;
use rqrr::PreparedImage;

/// Encodes an arbitrary string (e.g. a share link) into an SVG QR code string
pub fn encode_qr_svg(content: &str) -> ParseResult<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(ParseError::EmptyInput);
    }
    let code = QrCode::new(trimmed.as_bytes()).map_err(|e| ParseError::QrCode(e.to_string()))?;
    let svg_str = code
        .render()
        .min_dimensions(256, 256)
        .dark_color(svg::Color("#111827"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(svg_str)
}

/// Encodes a `ProxyNode`'s share link into an SVG QR code string
pub fn encode_node_qr_svg(node: &ProxyNode) -> ParseResult<String> {
    let uri = node_to_share_link(node)?;
    encode_qr_svg(&uri)
}

/// Encodes a string into a PNG image buffer containing the QR code (useful for round-trip tests and file export)
pub fn encode_qr_png_bytes(content: &str) -> ParseResult<Vec<u8>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(ParseError::EmptyInput);
    }
    let code = QrCode::new(trimmed.as_bytes()).map_err(|e| ParseError::QrCode(e.to_string()))?;
    let luma_img = code
        .render::<image::Luma<u8>>()
        .min_dimensions(256, 256)
        .build();
    let dyn_img = DynamicImage::ImageLuma8(luma_img);
    let mut buf = std::io::Cursor::new(Vec::new());
    dyn_img
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| ParseError::QrCode(e.to_string()))?;
    Ok(buf.into_inner())
}

/// Decodes all QR code payloads found in an image byte buffer (PNG, JPEG, BMP, WebP)
pub fn decode_qr_from_image_bytes(image_bytes: &[u8]) -> ParseResult<Vec<String>> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| ParseError::QrCode(format!("failed to load image: {e}")))?;
    let gray = img.to_luma8();
    decode_qr_from_gray_image(gray)
}

/// Decodes all QR code payloads found in an 8-bit grayscale image
pub fn decode_qr_from_gray_image(gray: GrayImage) -> ParseResult<Vec<String>> {
    let mut prepared = PreparedImage::prepare(gray);
    let grids = prepared.detect_grids();
    let mut results = Vec::new();

    for grid in grids {
        if let Ok((_meta, content)) = grid.decode() {
            let trimmed = content.trim().to_string();
            if !trimmed.is_empty() && !results.contains(&trimmed) {
                results.push(trimmed);
            }
        }
    }

    if results.is_empty() {
        return Err(ParseError::QrCode(
            "No valid QR code found in image".to_string(),
        ));
    }

    Ok(results)
}

/// Captures the current desktop screen and decodes any visible QR codes
pub fn decode_qr_from_screen() -> ParseResult<Vec<String>> {
    #[cfg(target_os = "windows")]
    {
        let gray = win32_screen_capture::capture_virtual_screen_gray()?;
        decode_qr_from_gray_image(gray)
    }
    #[cfg(target_os = "macos")]
    {
        let tmp = std::env::temp_dir().join(format!("novaproxy_qr_{}.png", uuid::Uuid::new_v4()));
        let status = std::process::Command::new("screencapture")
            .args(["-x", "-t", "png"])
            .arg(&tmp)
            .status()
            .map_err(|e| ParseError::QrCode(format!("screencapture failed: {e}")))?;
        let bytes = std::fs::read(&tmp)
            .map_err(|e| ParseError::QrCode(format!("Failed to read screenshot: {e}")));
        let _ = std::fs::remove_file(&tmp);
        if !status.success() {
            return Err(ParseError::QrCode(
                "macOS screen capture failed".to_string(),
            ));
        }
        decode_qr_from_image_bytes(&bytes?)
    }
    #[cfg(target_os = "linux")]
    {
        let tmp = std::env::temp_dir().join(format!("novaproxy_qr_{}.png", uuid::Uuid::new_v4()));
        let tmp_str = tmp.to_string_lossy().to_string();
        let candidates: [(&str, Vec<&str>); 4] = [
            ("grim", vec![&tmp_str]),
            ("gnome-screenshot", vec!["-f", &tmp_str]),
            ("scrot", vec![&tmp_str]),
            ("import", vec!["-window", "root", &tmp_str]),
        ];
        let mut captured = false;
        for (tool, args) in candidates {
            if let Ok(st) = std::process::Command::new(tool).args(&args).status() {
                if st.success() && tmp.exists() {
                    captured = true;
                    break;
                }
            }
        }
        if !captured {
            let _ = std::fs::remove_file(&tmp);
            return Err(ParseError::QrCode(
                "No Linux screen capture tool succeeded (install grim, gnome-screenshot, scrot, or imagemagick)".to_string(),
            ));
        }
        let bytes = std::fs::read(&tmp)
            .map_err(|e| ParseError::QrCode(format!("Failed to read screenshot: {e}")));
        let _ = std::fs::remove_file(&tmp);
        decode_qr_from_image_bytes(&bytes?)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(ParseError::QrCode(
            "Screen QR capture is not available on this platform; please import an image or paste a link".to_string(),
        ))
    }
}

#[cfg(target_os = "windows")]
mod win32_screen_capture {
    use super::*;
    use std::ffi::c_void;
    use std::ptr::null_mut;

    const SM_XVIRTUALSCREEN: i32 = 76;
    const SM_YVIRTUALSCREEN: i32 = 77;
    const SM_CXVIRTUALSCREEN: i32 = 78;
    const SM_CYVIRTUALSCREEN: i32 = 79;
    const SRCCOPY: u32 = 0x00CC0020;
    const BI_RGB: u32 = 0;
    const DIB_RGB_COLORS: u32 = 0;

    #[repr(C)]
    struct BitmapInfoHeader {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(n_index: i32) -> i32;
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(hdc: *mut c_void) -> *mut c_void;
        fn DeleteDC(hdc: *mut c_void) -> i32;
        fn CreateCompatibleBitmap(hdc: *mut c_void, cx: i32, cy: i32) -> *mut c_void;
        fn SelectObject(hdc: *mut c_void, h: *mut c_void) -> *mut c_void;
        fn DeleteObject(ho: *mut c_void) -> i32;
        fn BitBlt(
            hdc: *mut c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            hdc_src: *mut c_void,
            x1: i32,
            y1: i32,
            rop: u32,
        ) -> i32;
        fn GetDIBits(
            hdc: *mut c_void,
            hbm: *mut c_void,
            start: u32,
            c_lines: u32,
            lpv_bits: *mut c_void,
            lpbmi: *mut BitmapInfoHeader,
            usage: u32,
        ) -> i32;
    }

    pub fn capture_virtual_screen_gray() -> ParseResult<GrayImage> {
        unsafe {
            let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

            if width <= 0 || height <= 0 {
                return Err(ParseError::QrCode(
                    "Virtual screen dimensions are zero".to_string(),
                ));
            }

            let hdc_screen = GetDC(null_mut());
            if hdc_screen.is_null() {
                return Err(ParseError::QrCode("GetDC(NULL) failed".to_string()));
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.is_null() {
                ReleaseDC(null_mut(), hdc_screen);
                return Err(ParseError::QrCode("CreateCompatibleDC failed".to_string()));
            }

            let hbm = CreateCompatibleBitmap(hdc_screen, width, height);
            if hbm.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(null_mut(), hdc_screen);
                return Err(ParseError::QrCode(
                    "CreateCompatibleBitmap failed".to_string(),
                ));
            }

            let old_obj = SelectObject(hdc_mem, hbm);
            let blt_ok = BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, x, y, SRCCOPY);

            let mut bmi = BitmapInfoHeader {
                bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                bi_width: width,
                // Negative height requests top-down DIB
                bi_height: -height,
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: BI_RGB,
                bi_size_image: 0,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            };

            let pixel_count = (width as usize) * (height as usize);
            let mut bgra = vec![0u8; pixel_count * 4];

            let lines = if blt_ok != 0 {
                GetDIBits(
                    hdc_mem,
                    hbm,
                    0,
                    height as u32,
                    bgra.as_mut_ptr() as *mut c_void,
                    &mut bmi,
                    DIB_RGB_COLORS,
                )
            } else {
                0
            };

            SelectObject(hdc_mem, old_obj);
            DeleteObject(hbm);
            DeleteDC(hdc_mem);
            ReleaseDC(null_mut(), hdc_screen);

            if lines == 0 {
                return Err(ParseError::QrCode(
                    "Failed to capture screen bitmap bits".to_string(),
                ));
            }

            let mut luma = Vec::with_capacity(pixel_count);
            for chunk in bgra.as_chunks::<4>().0 {
                let b = chunk[0] as u32;
                let g = chunk[1] as u32;
                let r = chunk[2] as u32;
                let y_val = ((r * 77 + g * 150 + b * 29) >> 8) as u8;
                luma.push(y_val);
            }

            GrayImage::from_raw(width as u32, height as u32, luma).ok_or_else(|| {
                ParseError::QrCode("Failed to construct GrayImage from screen capture".to_string())
            })
        }
    }
}
