# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-22

### Added

- Initial release.
- `slice` — extract the substring covering display columns `[start, end)`,
  wide-character (CJK) and ANSI-escape aware, with active SGR styling re-applied
  and reset.
- `slice_from` — slice from a start column to the end of the string.
- `width` — display width of a string in terminal columns (ANSI-ignored).
- Single dependency (`unicode-width`); `#![no_std]` (requires `alloc`).

[0.1.0]: https://github.com/trananhtung/slice-ansi/releases/tag/v0.1.0
