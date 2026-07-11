//! Kitty-graphics placement registry for inline preview images.
//!
//! `ui.rs` queues (path, x, y, w_cells, rows) requests each frame; after the
//! ratatui draw the registry diffs them against what's on screen — decoding
//! and resizing once per (path, width), re-placing on scroll, and deleting
//! anything no longer requested so mode switches can't strand ghost images.

use std::collections::HashMap;
use std::io::{self, Write};

use xei_core::media::ImageAsset;
use xei_core::App;

use crate::gpu_frame;
use crate::kitty_gfx;
use crate::term_caps::TerminalCaps;

struct Placed {
    asset: ImageAsset,
    /// Last placement (x, y) — re-place only when it moves.
    at: Option<(u16, u16)>,
    used: bool,
}

pub struct GfxRegistry {
    /// key: "path|w_cells"
    items: HashMap<String, Placed>,
    next_id: u32,
}

impl GfxRegistry {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            next_id: 300,
        }
    }

    /// Reconcile requested placements with the screen. Returns true when any
    /// stdout write happened (caller re-asserts the caret).
    pub fn flush(
        &mut self,
        app: &mut App,
        caps: &TerminalCaps,
        editor_cursor: Option<(u16, u16)>,
    ) -> bool {
        let enabled = kitty_gfx::available(app.gpu_acc && app.gpu_graphics, caps);
        if !enabled && self.items.is_empty() {
            return false;
        }
        let requests: Vec<(String, u16, u16, u16, u16)> = if enabled {
            std::mem::take(&mut app.preview_gfx)
        } else {
            app.preview_gfx.clear();
            Vec::new()
        };

        for p in self.items.values_mut() {
            p.used = false;
        }

        let mut painted = false;
        let use_sync = gpu_frame::should_sync(app.gpu_acc, caps);
        let cell_px = app.cell_px_or_default();
        let mut out = io::stdout();

        for (path, x, y, w_cells, _rows) in requests {
            let key = format!("{path}|{w_cells}");
            if !self.items.contains_key(&key) {
                let Ok(mut asset) = ImageAsset::load(std::path::Path::new(&path), cell_px)
                else {
                    continue;
                };
                asset.width_cells = w_cells;
                asset.kitty_id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1).max(300);
                asset.rebuild_cache(cell_px);
                self.items.insert(
                    key.clone(),
                    Placed {
                        asset,
                        at: None,
                        used: false,
                    },
                );
            }
            let Some(item) = self.items.get_mut(&key) else {
                continue;
            };
            item.used = true;
            if item.at == Some((x, y)) {
                continue; // already exactly there
            }
            let a = &item.asset;
            if kitty_gfx::place_rgba_rect_b64(
                &mut out,
                a.kitty_id,
                a.cached_w,
                a.cached_h,
                Some(&a.cached_b64),
                &a.cached_rgba,
                x as u32,
                y as u32,
                editor_cursor,
                use_sync,
            )
            .is_ok()
            {
                item.at = Some((x, y));
                painted = true;
            }
        }

        // Anything not requested this frame disappears (scroll-out, close,
        // mode switch, resize).
        let stale: Vec<String> = self
            .items
            .iter()
            .filter(|(_, p)| !p.used)
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            if let Some(p) = self.items.remove(&k) {
                let _ = kitty_gfx::delete_image_flush(&mut out, p.asset.kitty_id);
                painted = true;
            }
        }
        let _ = out.flush();
        painted
    }
}
