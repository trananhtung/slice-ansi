//! # slice-ansi — slice strings by terminal display column
//!
//! Extract a substring covering a range of terminal **display columns**, correctly
//! handling **wide (CJK) characters** (two columns) and **ANSI escape sequences**
//! (zero columns). Styling active at the cut is re-applied to the slice and reset
//! at the end, so the result renders the same as that part of the original. A Rust
//! take on Node's [`slice-ansi`](https://www.npmjs.com/package/slice-ansi).
//!
//! ```
//! use slice_ansi::{slice, width};
//! assert_eq!(slice("hello", 1, 3), "el");
//! assert_eq!(slice("古池や蛙", 0, 4), "古池");                 // wide chars
//! assert_eq!(slice("\u{1b}[31mhello\u{1b}[0m", 1, 3), "\u{1b}[31mel\u{1b}[0m");
//! assert_eq!(width("\u{1b}[31mhi\u{1b}[0m"), 2);                // ANSI = zero width
//! ```
//!
//! ## Behavior
//!
//! - Columns are half-open `[start, end)`; `end` is clamped to the string width and
//!   `start >= end` yields `""`. Output width never exceeds `end - start`.
//! - A wide character that straddles either boundary is excluded, so the result is
//!   never a broken half-glyph. Combining marks travel with their base character.
//! - SGR styling active at `start` is re-emitted at the front of the slice, and a
//!   reset (`\x1b[0m`) is appended when styling is still open at the end — color
//!   can't leak, and a reset already inside the slice isn't duplicated. An OSC 8
//!   hyperlink open at the cut is likewise re-opened and closed so links stay balanced.
//! - Escape sequences (CSI, OSC, DCS/etc., two-byte/nF escapes, 8-bit C1) are
//!   recognized as zero width and never split.
//! - Limitations: attribute-off SGR codes (`39`/`49`, `22`–`29`) aren't collapsed,
//!   so a slice may carry a redundant (but correctly rendering) style; and a cut
//!   *inside* a multi-codepoint grapheme cluster (a ZWJ emoji) may split it.

#![no_std]
#![doc(html_root_url = "https://docs.rs/slice-ansi/0.1.0")]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use unicode_width::UnicodeWidthChar;

/// The SGR sequence that resets all styling to its default.
const RESET: &str = "\u{1b}[0m";
/// The OSC 8 sequence that closes an open hyperlink.
const OSC8_CLOSE: &str = "\u{1b}]8;;\u{1b}\\";

/// The display width of `text` in terminal columns, ignoring escape sequences and
/// counting wide characters as two.
#[must_use]
pub fn width(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut total = 0;
    let mut i = 0;
    while i < chars.len() {
        let (scanned, next) = scan(&chars, i);
        if let Scanned::Visible(c) = scanned {
            total += char_width(c);
        }
        i = next;
    }
    total
}

/// Slice `text` to the display columns `[start, end)`, preserving ANSI styling.
///
/// `end` is clamped to the string's display width; `start >= end` returns `""`.
///
/// ```
/// assert_eq!(slice_ansi::slice("hello", 1, 4), "ell");
/// ```
#[must_use]
pub fn slice(text: &str, start: usize, end: usize) -> String {
    if start >= end {
        return String::new();
    }

    let toks = tokenize(text);

    // Locate the visible glyphs fully inside [start, end), and the SGR + hyperlink
    // state at the first kept glyph (so styling opened before the slice carries in).
    let mut col = 0;
    let mut sgr: Vec<String> = Vec::new();
    let mut link: Option<String> = None;
    let mut lo: Option<usize> = None;
    let mut hi = 0;
    let mut sgr_at_lo: Vec<String> = Vec::new();
    let mut link_at_lo: Option<String> = None;
    for (idx, t) in toks.iter().enumerate() {
        match t {
            Tok::Esc(s) => {
                update_sgr(&mut sgr, s);
                update_link(&mut link, s);
            }
            Tok::Ch(_, w) => {
                let (cs, ce) = (col, col + w);
                if cs >= end {
                    break;
                }
                if cs >= start && ce <= end {
                    if lo.is_none() {
                        lo = Some(idx);
                        sgr_at_lo.clone_from(&sgr);
                        link_at_lo.clone_from(&link);
                    }
                    hi = idx + 1;
                }
                col = ce;
            }
        }
    }

    let Some(lo) = lo else {
        return String::new();
    };

    let mut out = String::new();
    if let Some(l) = &link_at_lo {
        out.push_str(l);
    }
    for code in &sgr_at_lo {
        out.push_str(code);
    }
    let mut open_sgr = sgr_at_lo;
    let mut open_link = link_at_lo;
    for t in &toks[lo..hi] {
        match t {
            Tok::Esc(s) => {
                out.push_str(s);
                update_sgr(&mut open_sgr, s);
                update_link(&mut open_link, s);
            }
            Tok::Ch(s, _) => out.push_str(s),
        }
    }
    if !open_sgr.is_empty() {
        out.push_str(RESET);
    }
    if open_link.is_some() {
        out.push_str(OSC8_CLOSE);
    }
    out
}

/// Slice `text` from display column `start` to the end of the string.
///
/// ```
/// assert_eq!(slice_ansi::slice_from("hello", 2), "llo");
/// ```
#[must_use]
pub fn slice_from(text: &str, start: usize) -> String {
    slice(text, start, width(text))
}

// ---------------------------------------------------------------------------
// SGR / hyperlink state tracking
// ---------------------------------------------------------------------------

/// Update the active SGR stack from an escape. A reset parameter anywhere in the
/// sequence clears the stack; a sequence that also sets styles after its last reset
/// is kept (it self-resets on re-emit); other SGR sequences are pushed.
fn update_sgr(active: &mut Vec<String>, seq: &str) {
    let Some(params) = sgr_params(seq) else {
        return;
    };
    let mut last_reset = None;
    let mut count = 0;
    for (i, p) in params.split(';').enumerate() {
        if is_reset_param(p) {
            last_reset = Some(i);
        }
        count = i + 1;
    }
    match last_reset {
        Some(idx) => {
            active.clear();
            if idx + 1 < count {
                active.push(seq.to_string());
            }
        }
        None => active.push(seq.to_string()),
    }
}

/// Whether an SGR parameter is a reset (empty, or all zeros).
fn is_reset_param(p: &str) -> bool {
    p.is_empty() || p.bytes().all(|b| b == b'0')
}

/// The parameter substring of an SGR sequence (`CSI … m`), or `None` if not SGR.
fn sgr_params(seq: &str) -> Option<&str> {
    let body = match seq.strip_prefix('\u{1b}') {
        Some(rest) => rest.strip_prefix('[')?,
        None => seq.strip_prefix('\u{9b}')?,
    };
    body.strip_suffix('m')
}

/// Update the active hyperlink from an escape: an OSC 8 open sets it, a close
/// clears it, and any other escape leaves it unchanged.
fn update_link(link: &mut Option<String>, seq: &str) {
    match osc8_kind(seq) {
        Some(true) => *link = Some(seq.to_string()),
        Some(false) => *link = None,
        None => {}
    }
}

/// Classify an OSC 8 hyperlink sequence: `Some(true)` if it opens a link (non-empty
/// URI), `Some(false)` if it closes one (empty URI), `None` if not an OSC 8 sequence.
fn osc8_kind(seq: &str) -> Option<bool> {
    let body = match seq.strip_prefix('\u{1b}') {
        Some(rest) => rest.strip_prefix(']')?,
        None => seq.strip_prefix('\u{9d}')?,
    };
    let body = body.strip_prefix("8;")?;
    let body = body
        .strip_suffix('\u{07}')
        .or_else(|| body.strip_suffix("\u{1b}\\"))
        .or_else(|| body.strip_suffix('\u{9c}'))?;
    let uri = body.split_once(';')?.1;
    Some(!uri.is_empty())
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

/// A parsed token: a zero-width escape sequence (preserved verbatim) or a visible
/// glyph (a base char plus any trailing combining marks) with its column width.
enum Tok {
    Esc(String),
    Ch(String, usize),
}

/// Tokenize `text`, preserving escape sequences, dropping control characters, and
/// folding trailing zero-width chars (combining marks, ZWJ) into the preceding glyph.
fn tokenize(text: &str) -> Vec<Tok> {
    let chars: Vec<char> = text.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (scanned, next) = scan(&chars, i);
        match scanned {
            Scanned::Escape(seq) => {
                toks.push(Tok::Esc(seq));
                i = next;
            }
            Scanned::Control => i = next,
            Scanned::Visible(c) => {
                let w = char_width(c);
                let mut glyph = String::new();
                glyph.push(c);
                i = next;
                // Absorb following zero-width visible chars into this glyph.
                while i < chars.len() {
                    let (sc, nx) = scan(&chars, i);
                    match sc {
                        Scanned::Visible(c2) if char_width(c2) == 0 => {
                            glyph.push(c2);
                            i = nx;
                        }
                        _ => break,
                    }
                }
                toks.push(Tok::Ch(glyph, w));
            }
        }
    }
    toks
}

/// Display width of a single visible char (zero-width chars → 0).
fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Escape / control scanner (single source of truth for width() and tokenize())
// ---------------------------------------------------------------------------

/// The classification of the token starting at a given index.
enum Scanned {
    /// A complete escape sequence (zero display width), preserved verbatim.
    Escape(String),
    /// A visible character.
    Visible(char),
    /// A C0/C1 control character with no defined column width.
    Control,
}

/// Scan one token starting at `chars[i]`, returning it and the index just past it.
/// Always advances (`next > i`).
fn scan(chars: &[char], i: usize) -> (Scanned, usize) {
    let c = chars[i];
    if c == '\u{1b}' {
        return scan_escape(chars, i);
    }
    match c {
        '\u{9b}' => return scan_csi(chars, i + 1, i),
        '\u{90}' | '\u{9d}' | '\u{98}' | '\u{9e}' | '\u{9f}' => {
            return scan_string_sequence(chars, i + 1, i);
        }
        _ => {}
    }
    if c.is_control() {
        return (Scanned::Control, i + 1);
    }
    (Scanned::Visible(c), i + 1)
}

/// Scan a 7-bit escape beginning with `ESC` at `chars[i]`.
fn scan_escape(chars: &[char], i: usize) -> (Scanned, usize) {
    match chars.get(i + 1).copied() {
        Some('[') => scan_csi(chars, i + 2, i),
        Some(']' | 'P' | 'X' | '^' | '_') => scan_string_sequence(chars, i + 2, i),
        Some(c) if ('\u{20}'..='\u{2f}').contains(&c) => {
            let mut j = i + 2;
            while j < chars.len() && ('\u{20}'..='\u{2f}').contains(&chars[j]) {
                j += 1;
            }
            if j < chars.len() && ('\u{30}'..='\u{7e}').contains(&chars[j]) {
                j += 1;
            }
            (Scanned::Escape(collect(chars, i, j)), j)
        }
        Some(c) if ('\u{30}'..='\u{7e}').contains(&c) => {
            (Scanned::Escape(collect(chars, i, i + 2)), i + 2)
        }
        _ => (Scanned::Escape(collect(chars, i, i + 1)), i + 1),
    }
}

/// Scan a CSI body (params 0x30–0x3F, intermediates 0x20–0x2F, final 0x40–0x7E),
/// starting at `body` and reporting the sequence from `start`.
fn scan_csi(chars: &[char], body: usize, start: usize) -> (Scanned, usize) {
    let mut j = body;
    while j < chars.len() && ('\u{30}'..='\u{3f}').contains(&chars[j]) {
        j += 1;
    }
    while j < chars.len() && ('\u{20}'..='\u{2f}').contains(&chars[j]) {
        j += 1;
    }
    if j < chars.len() && ('\u{40}'..='\u{7e}').contains(&chars[j]) {
        j += 1;
    }
    (Scanned::Escape(collect(chars, start, j)), j)
}

/// Scan a string sequence (OSC/DCS/SOS/PM/APC) up to and including its terminator —
/// `BEL` (0x07), 8-bit `ST` (0x9C), or 7-bit `ST` (`ESC \`).
fn scan_string_sequence(chars: &[char], body: usize, start: usize) -> (Scanned, usize) {
    let mut j = body;
    while j < chars.len() {
        let c = chars[j];
        if c == '\u{07}' || c == '\u{9c}' {
            j += 1;
            break;
        }
        if c == '\u{1b}' && chars.get(j + 1) == Some(&'\\') {
            j += 2;
            break;
        }
        j += 1;
    }
    (Scanned::Escape(collect(chars, start, j)), j)
}

/// Collect `chars[start..end]` into a `String`.
fn collect(chars: &[char], start: usize, end: usize) -> String {
    chars[start..end].iter().collect()
}
