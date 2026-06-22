//! End-to-end behavioral spec for the public `slice-ansi` API.

use slice_ansi::{slice, slice_from, width};

// ---------------------------------------------------------------------------
// width()
// ---------------------------------------------------------------------------

#[test]
fn width_counts_display_columns() {
    assert_eq!(width("hello"), 5);
    assert_eq!(width(""), 0);
    assert_eq!(width("古"), 2); // wide (CJK) = 2 columns
    assert_eq!(width("古池"), 4);
    assert_eq!(width("\u{1b}[31mhi\u{1b}[0m"), 2); // ANSI escapes are zero-width
}

// ---------------------------------------------------------------------------
// slice() — plain text
// ---------------------------------------------------------------------------

#[test]
fn slice_plain_text() {
    assert_eq!(slice("hello", 0, 3), "hel");
    assert_eq!(slice("hello", 1, 3), "el");
    assert_eq!(slice("hello", 2, 100), "llo"); // end clamped to width
    assert_eq!(slice("hello", 0, 5), "hello");
}

#[test]
fn slice_empty_ranges() {
    assert_eq!(slice("hello", 3, 3), ""); // start == end
    assert_eq!(slice("hello", 4, 2), ""); // start > end
    assert_eq!(slice("hello", 9, 12), ""); // start past width
    assert_eq!(slice("", 0, 5), "");
}

// ---------------------------------------------------------------------------
// slice() — wide characters
// ---------------------------------------------------------------------------

#[test]
fn slice_respects_wide_chars() {
    assert_eq!(slice("古池や", 0, 4), "古池"); // 古[0,2) 池[2,4) fit; や[4,6) out
    assert_eq!(width(&slice("古池や", 0, 4)), 4);
}

#[test]
fn slice_excludes_wide_char_straddling_a_boundary() {
    // start=1 cuts the first wide char 古[0,2); it is excluded, slice begins at 池
    assert_eq!(slice("古池や", 1, 5), "池");
    // end=3 cuts 池[2,4); it is excluded
    assert_eq!(slice("古池や", 0, 3), "古");
    assert!(width(&slice("古池や", 1, 5)) <= 4);
}

// ---------------------------------------------------------------------------
// slice() — ANSI styling carried across the cut
// ---------------------------------------------------------------------------

#[test]
fn slice_reapplies_active_style_and_resets() {
    // The red SGR opens before the slice; the slice must re-apply it and reset.
    assert_eq!(
        slice("\u{1b}[31mhello\u{1b}[0m", 1, 3),
        "\u{1b}[31mel\u{1b}[0m"
    );
}

#[test]
fn slice_carries_multiple_active_styles() {
    assert_eq!(
        slice("\u{1b}[31m\u{1b}[1mhello\u{1b}[0m", 0, 2),
        "\u{1b}[31m\u{1b}[1mhe\u{1b}[0m"
    );
}

#[test]
fn slice_keeps_in_range_reset_and_adds_no_extra() {
    // Style resets inside the slice → no synthetic trailing reset needed.
    assert_eq!(
        slice("\u{1b}[31mab\u{1b}[0mcd", 1, 4),
        "\u{1b}[31mb\u{1b}[0mcd"
    );
}

#[test]
fn slice_unstyled_gets_no_reset() {
    assert_eq!(slice("hello", 1, 3), "el");
    assert!(!slice("hello", 1, 3).contains('\u{1b}'));
}

// ---------------------------------------------------------------------------
// slice_from()
// ---------------------------------------------------------------------------

#[test]
fn slice_from_to_end() {
    assert_eq!(slice_from("hello", 2), "llo");
    assert_eq!(
        slice_from("\u{1b}[31mhello\u{1b}[0m", 2),
        "\u{1b}[31mllo\u{1b}[0m"
    );
    assert_eq!(slice_from("hello", 0), "hello");
    assert_eq!(slice_from("hello", 9), "");
}

// ---------------------------------------------------------------------------
// Output never exceeds the requested column span
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Regression: adversarial-review findings
// ---------------------------------------------------------------------------

#[test]
fn combining_marks_stay_with_their_base_glyph() {
    // F1/F2: a combining mark must travel with its base char, never orphan.
    assert_eq!(slice("e\u{301}x", 0, 1), "e\u{301}"); // é kept whole
    assert_eq!(slice("e\u{301}x", 1, 2), "x"); // accent not orphaned onto x
    assert_eq!(slice("ab\u{301}", 0, 2), "ab\u{301}"); // trailing accent kept
    assert_eq!(slice("a\u{301}bc", 1, 3), "bc"); // leading base dropped → no orphan
}

#[test]
fn embedded_sgr_reset_clears_style() {
    // SGR-1: a reset param anywhere (1;0m) clears style → no spurious trailing reset.
    assert_eq!(slice("\u{1b}[1;0mab", 0, 2), "ab");
    assert_eq!(
        slice("\u{1b}[31mab\u{1b}[1;0mcd", 0, 4),
        "\u{1b}[31mab\u{1b}[1;0mcd"
    );
    // 0;32m resets then sets green, which must be carried.
    assert_eq!(slice("\u{1b}[0;32mab", 0, 2), "\u{1b}[0;32mab\u{1b}[0m");
}

#[test]
fn osc8_hyperlinks_are_balanced() {
    // F1-scanner: a slice cutting a hyperlink must stay balanced.
    let link = "AB\u{1b}]8;;http://x\u{1b}\\CD\u{1b}]8;;\u{1b}\\EF";
    // [0,3) contains the open but not the close → append a synthetic close.
    let out = slice(link, 0, 3);
    assert!(out.starts_with("AB\u{1b}]8;;http://x\u{1b}\\C"), "{out:?}");
    assert!(out.ends_with("\u{1b}]8;;\u{1b}\\"), "{out:?}");
    // [3,5) starts inside the link → re-open it, then close where the original did.
    let out = slice(link, 3, 5);
    assert!(out.starts_with("\u{1b}]8;;http://x\u{1b}\\D"), "{out:?}");
    assert!(out.contains("\u{1b}]8;;\u{1b}\\E"), "{out:?}");
}

#[test]
fn slice_width_never_exceeds_span() {
    let s = "a古b池c蛙d";
    for start in 0..=width(s) {
        for end in start..=width(s) + 2 {
            let out = slice(s, start, end);
            assert!(
                width(&out) <= end - start,
                "slice({start},{end}) = {out:?} has width {}",
                width(&out)
            );
        }
    }
}
