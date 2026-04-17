//! Export translation transcripts to Markdown.
//!
//! ## Call site
//!
//! `offer_save_dialog` must be called from the **main thread** (i.e. a Makepad
//! event handler), because macOS native dialogs (NSAlert / NSOpenPanel) require
//! the main run loop to be running. Only the actual file write is dispatched to
//! a background thread so the UI is not blocked by I/O.

use std::path::Path;

/// Show a confirmation dialog then a file-save dialog, then write the file.
///
/// **Must be called from the main thread.**
/// No-op when `entries` is empty.
pub fn offer_save_dialog(
    entries: Vec<(String, String)>,
    src_lang: String,
    tgt_lang: String,
    locale_en: bool,
) {
    if entries.is_empty() {
        return;
    }

    let count = entries.len();

    // Confirmation — runs synchronously on the calling (main) thread.
    let (title, desc) = if locale_en {
        (
            "Save Transcript",
            format!("{count} sentences recorded. Save as transcript?"),
        )
    } else {
        (
            "保存发言稿",
            format!("本次翻译共 {count} 条记录，是否保存发言稿？"),
        )
    };

    let confirmed = rfd::MessageDialog::new()
        .set_title(title)
        .set_description(&desc)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();

    if confirmed != rfd::MessageDialogResult::Yes {
        return;
    }

    // File picker — also on main thread.
    let save_title = if locale_en { "Save Transcript" } else { "保存发言稿" };
    let path = rfd::FileDialog::new()
        .set_title(save_title)
        .add_filter("Markdown", &["md"])
        .set_file_name("transcript.md")
        .save_file();

    let Some(path) = path else { return; };

    // Dispatch the (potentially slow) write to a background thread.
    std::thread::spawn(move || {
        match save_as_md(&path, &entries, &src_lang, &tgt_lang) {
            Ok(()) => log::info!("[transcript] Saved to {:?}", path),
            Err(e) => log::error!("[transcript] Failed to save: {}", e),
        }
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Approximate UTC datetime string from the system clock.
fn utc_datetime() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let day_sec = (secs % 86400) as u32;
    let h = day_sec / 3600;
    let m = (day_sec % 3600) / 60;

    let mut days = secs / 86400;
    let mut year = 1970u32;
    loop {
        let n = if is_leap(year) { 366u64 } else { 365u64 };
        if days < n {
            break;
        }
        days -= n;
        year += 1;
    }
    let month_lens: [u64; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    let mut month = 1u32;
    for &ml in &month_lens {
        if days < ml {
            break;
        }
        days -= ml;
        month += 1;
    }
    format!("{year:04}-{month:02}-{:02} {h:02}:{m:02} UTC", days + 1)
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ── Markdown ──────────────────────────────────────────────────────────────────

fn save_as_md(
    path: &Path,
    entries: &[(String, String)],
    src_lang: &str,
    tgt_lang: &str,
) -> std::io::Result<()> {
    let mut out = String::new();
    out.push_str("# Speech Transcript\n\n");
    out.push_str(&format!("**Time:** {}  \n", utc_datetime()));
    out.push_str(&format!("**Languages:** {src_lang} → {tgt_lang}\n\n"));
    out.push_str("---\n\n");

    for (i, (src, tl)) in entries.iter().enumerate() {
        out.push_str(&format!("**[{:03}]**\n\n", i + 1));
        out.push_str(&format!("> {}\n\n", src.trim()));
        out.push_str(&format!("{}\n\n", tl.trim()));
        out.push_str("---\n\n");
    }
    std::fs::write(path, out)
}
