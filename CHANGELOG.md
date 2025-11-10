## [0.5.0] - 2025-11-10
### Added
- Confidence gating with hysteresis (go-live ≥ 0.70, drop < 0.65) via a public wrapper.
- Desktop guard: skips hinge on desktops; ALS fallback path.
- Diagnostics line on successful init (enable with `BOOKLID_DIAGNOSTICS=1`).
- Examples show “waiting…” until gate passes and print `confidence()`.

### Changed
- Distinct source tagging for HID discovery (`HingeHid`).
- Clearer error on failure (includes `tried` chain).

### Policy
- Mock is strictly opt-in: requires `--features mock` + `allow_mock(true)`.
- Default probe chain never includes mock.

### Notes
- Blocking constructors use a global multithreaded Tokio runtime; avoid calling them from async contexts.
