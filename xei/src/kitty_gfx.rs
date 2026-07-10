//! Kitty graphics protocol helpers (Phase B overlays).
//!
//! **Cursor policy:** placement is wrapped in a synchronized update (CSI ?2026)
//! so intermediate CUP never becomes a visible blink. We never toggle cursor
//! visibility (`?25l`/`?25h`). After place we restore the editor caret.

#![allow(dead_code)]

use std::io::{self, Write};

use crate::term_caps::TerminalCaps;

/// Whether Kitty graphics may be used this session.
pub fn available(gpu_acc: bool, caps: &TerminalCaps) -> bool {
    gpu_acc && caps.kitty_graphics
}

const PLACEMENT_ID: u32 = 1;

pub fn delete_image(out: &mut impl Write, id: u32) -> io::Result<()> {
    write!(out, "\x1b_Ga=d,d=i,i={id},q=2\x1b\\")?;
    Ok(())
}

pub fn delete_image_flush(out: &mut impl Write, id: u32) -> io::Result<()> {
    delete_image(out, id)?;
    out.flush()
}

pub fn delete_placement(out: &mut impl Write, id: u32) -> io::Result<()> {
    write!(
        out,
        "\x1b_Ga=d,d=p,i={id},p={PLACEMENT_ID},q=2\x1b\\"
    )?;
    out.flush()
}

pub fn place_rgba_rect(
    out: &mut impl Write,
    id: u32,
    width_px: u32,
    height_px: u32,
    rgba: &[u8],
    col: u32,
    row: u32,
    restore_cursor: Option<(u16, u16)>,
) -> io::Result<()> {
    place_rgba_rect_b64(
        out,
        id,
        width_px,
        height_px,
        None,
        rgba,
        col,
        row,
        restore_cursor,
        true, // prefer sync when placing
    )
}

/// Place with optional precomputed base64.
///
/// `use_sync`: wrap in DEC synchronized update so CUP→pet is not painted mid-frame.
pub fn place_rgba_rect_b64(
    out: &mut impl Write,
    id: u32,
    width_px: u32,
    height_px: u32,
    b64_cached: Option<&str>,
    rgba: &[u8],
    col: u32,
    row: u32,
    restore_cursor: Option<(u16, u16)>,
    use_sync: bool,
) -> io::Result<()> {
    if b64_cached.is_none() && rgba.len() != (width_px * height_px * 4) as usize {
        return Ok(());
    }
    let owned;
    let b64: &str = if let Some(s) = b64_cached {
        s
    } else {
        owned = base64_encode(rgba);
        owned.as_str()
    };

    const CHUNK: usize = 3840;
    let mut offset = 0;
    let total = b64.len();
    let mut first = true;

    // Synchronized update: host holds the frame until end — no visible cursor jump.
    if use_sync {
        write!(out, "\x1b[?2026h")?;
    }

    // Save → jump to image cell → transmit (C=1) → restore caret.
    // All inside the sync region so nothing is shown until the caret is back.
    write!(out, "\x1b7")?; // DECSC
    write!(
        out,
        "\x1b[{};{}H",
        row.saturating_add(1),
        col.saturating_add(1)
    )?;

    while offset < total {
        let end = (offset + CHUNK).min(total);
        let more = if end < total { 1 } else { 0 };
        let slice = &b64[offset..end];
        if first {
            write!(
                out,
                "\x1b_Ga=T,f=32,s={width_px},v={height_px},i={id},p={PLACEMENT_ID},q=2,C=1,m={more};{slice}\x1b\\"
            )?;
            first = false;
        } else {
            write!(out, "\x1b_Gm={more};{slice}\x1b\\")?;
        }
        offset = end;
    }

    write!(out, "\x1b8")?; // DECRC
    if let Some((rc, rr)) = restore_cursor {
        write!(
            out,
            "\x1b[{};{}H",
            (rr as u32).saturating_add(1),
            (rc as u32).saturating_add(1)
        )?;
    }

    if use_sync {
        write!(out, "\x1b[?2026l")?;
    }
    out.flush()
}

pub fn encode_base64(data: &[u8]) -> String {
    base64_encode(data)
}

pub fn place_shadow_bar(
    out: &mut impl Write,
    id: u32,
    cols: u32,
    cell_px: u32,
    col: u32,
    row: u32,
) -> io::Result<()> {
    let w = cols.saturating_mul(cell_px).max(8);
    let h = 6u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let a = 40u8.saturating_add((y * 12) as u8);
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            rgba[i] = 0;
            rgba[i + 1] = 0;
            rgba[i + 2] = 0;
            rgba[i + 3] = a;
        }
    }
    place_rgba_rect_b64(out, id, w, h, None, &rgba, col, row, None, false)
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    if i < data.len() {
        let rem = data.len() - i;
        let n = if rem == 1 {
            (data[i] as u32) << 16
        } else {
            ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8)
        };
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        if rem == 1 {
            out.push('=');
            out.push('=');
        } else {
            out.push(T[((n >> 6) & 63) as usize] as char);
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_hello() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
}
