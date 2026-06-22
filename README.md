# slice-ansi

[![Crates.io](https://img.shields.io/crates/v/slice-ansi.svg)](https://crates.io/crates/slice-ansi)
[![Documentation](https://docs.rs/slice-ansi/badge.svg)](https://docs.rs/slice-ansi)
[![CI](https://github.com/trananhtung/slice-ansi/actions/workflows/ci.yml/badge.svg)](https://github.com/trananhtung/slice-ansi/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/slice-ansi.svg)](#license)

**Slice a string by terminal display column** — correctly handling **wide (CJK)
characters** (two columns) and **ANSI escape sequences** (zero columns). Styling
active at the cut is re-applied to the slice and reset at the end, so the result
renders exactly like that part of the original. A Rust take on Node's
[`slice-ansi`](https://www.npmjs.com/package/slice-ansi).

```rust
use slice_ansi::{slice, slice_from, width};

assert_eq!(slice("hello", 1, 3), "el");
assert_eq!(slice("古池や蛙", 0, 4), "古池");                       // wide chars
assert_eq!(slice("\u{1b}[31mhello\u{1b}[0m", 1, 3), "\u{1b}[31mel\u{1b}[0m"); // style preserved
assert_eq!(width("\u{1b}[31mhi\u{1b}[0m"), 2);                     // ANSI = zero width
```

## Why slice-ansi?

`ansi-width` measures a styled string's width, but there's no crate to *extract* a
column range from one. Naively slicing bytes splits escape sequences and wide
characters and drops the active color. `slice-ansi` is the focused piece — pull a
window out of a styled, wide-character line for tables, columns, scrolling viewports,
and TUIs. It pairs with [`cli-truncate`](https://crates.io/crates/cli-truncate)
(which keeps a width-bounded prefix).

```toml
[dependencies]
slice-ansi = "0.1"
```

## API

| Item | Purpose |
| --- | --- |
| `slice(text, start, end)` | Substring covering display columns `[start, end)` |
| `slice_from(text, start)` | Slice from `start` to the end of the string |
| `width(text)` | Display width in columns (ANSI-ignored, wide chars = 2) |

## Behavior

- Columns are half-open `[start, end)`; `end` is clamped to the string width and
  `start >= end` yields `""`. Output width never exceeds `end - start`.
- A wide character that straddles either boundary is excluded, so the result is
  never a broken half-glyph.
- SGR styling active at `start` is re-emitted at the front of the slice, and a reset
  (`\x1b[0m`) is appended when styling is still open at the end — color can't leak,
  and a reset already inside the slice isn't duplicated. An OSC 8 hyperlink open at
  the cut is likewise re-opened and closed, so links stay balanced.
- Escape sequences (CSI, OSC, DCS/etc., two-byte/nF escapes, 8-bit C1) are
  recognized as zero width and never split. Combining marks travel with their base
  character.
- Limitations: attribute-off SGR codes (`39`/`49`, `22`–`29`) aren't collapsed, so a
  slice may carry a redundant (but correctly rendering) style; and a cut *inside* a
  multi-codepoint grapheme cluster (a ZWJ emoji) may split it.

## `no_std`

`#![no_std]` (needs only `alloc`); the single dependency is `unicode-width`.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
