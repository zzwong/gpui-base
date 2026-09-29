# gpui-base (patched for diffz)

This is the published [`gpui-base`](https://crates.io/crates/gpui-base) 0.6.0
crate from [gpui-kit](https://github.com/longbridge/gpui-kit), with one patch
carried for [diffz](https://github.com/zzwong/diffz) until the same change lands
upstream in longbridge/gpui-kit:

- **Stop the input cursor's blink timer when nobody is looking.** A focused
  input blinked its cursor every 500 ms for as long as it kept focus, and each
  blink redraws the window, so an idle app woke twice a second forever,
  including while its window was inactive and no cursor was drawn at all. Now
  the cursor settles visible, with no timer, after 5 s without input; any key,
  click or edit resumes the blink as before. Deactivating the window stops the
  blink and activating it restarts it, and focusing an input in an inactive
  window does not start it. `stop` also drops its pending timer and `pause` does
  nothing on a stopped cursor, as in gpui-base 0.7.0. See
  [diffz#69](https://github.com/zzwong/diffz/issues/69).

The blink tests in `src/input/base/blink_cursor.rs` and
`test_blink_cursor_runs_only_while_the_window_is_active` in
`src/input/base/state.rs` drive the timer with a fake clock and count the
repaint notifications: none after the idle delay, and none in an inactive
window.

`v0.6.0-upstream` tags the pristine published crate, so `git diff
v0.6.0-upstream` is the full carried delta. This repository is temporary and
will be archived once a gpui-base release ships the change.

Licensed under Apache-2.0, as the upstream crate is.
