## [1.0.0] - 2025-12-28

### Added

* **Windows and Linux backends completed**:

  * **Windows (WinRT)** probe chain: **Hinge → Tilt → ALS**.
  * **Linux** probe chain: **iio-sensor-proxy (DBus) → IIO `/sys` → hwmon/other fallbacks**, with bounds checks and NaN guards.
* **`OpenConfig` builder** (replaces `OpenOptions`):

  * Configure `hz`, `smoothing_alpha`, `min_confidence`, `prefer_sources`,
    `disable_backends`, `discovery`, `allow_mock`, `diagnostics`,
    `fail_after`, and `persistence`.
  * Validation with clear errors (e.g. invalid `hz`, prefer/disable conflicts).
* **Persistence support**:

  * Remembers the last successful backend (`Source`) and prefers it on the next startup (unless disabled).
  * Added `clear_persisted_state()` helper.
* **Stable, pattern-matchable backend error**:

  * `Error::NoBackend { tried }`.

### Changed

* **API stabilized for 1.x**:

  * Public surface: `AngleDevice`, `AngleSample`, `Source`,
    `open` / `open_blocking`,
    `open_with_config` / `open_blocking_with_config`.
* **Confidence gating is configurable**:

  * Threshold set via `OpenConfig.min_confidence`.
  * Drop threshold applies a small hysteresis below the minimum.

### Notes

* CI uses deterministic smoke runs via the `mock` backend.
* Hardware sensor smoke tests remain opt-in for local and development environments.

---

## [0.5.0] - 2025-11-10

### Added

* Confidence gating with hysteresis (go-live ≥ **0.70**, drop < **0.65**).
* Desktop guard: skips hinge backends on desktops; ALS fallback path.
* Diagnostics line on successful init (enable with `BOOKLID_DIAGNOSTICS=1`).
* Examples show “waiting…” until confidence passes and print `confidence()`.

### Changed

* Distinct source tagging for HID discovery (`HingeHid`).
* Clearer backend failure errors (include the `tried` probe chain).

### Policy

* Mock backend is **strictly opt-in**:

  * Requires `--features mock` and `allow_mock(true)`.
  * Default probe chains never include mock.

### Notes

* Blocking constructors use a global multithreaded Tokio runtime.
  Avoid calling them from async contexts.

---
