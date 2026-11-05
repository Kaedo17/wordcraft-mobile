# Pocket Mode

Pocket Mode is WordCraft's phone/tablet presentation: the document reflowed to a pocket's column
instead of laid out as pages. It is a first-class view state (`view.pocketMode`), a switch in
**View › Views**, and — on Android — the way the two front ends hand the session to each other.

*Status: the engine, the layout mode, the switch and the bridge are implemented and tested; the
Android hand-off is implemented and cross-compiles, with on-device verification outstanding.*

## What it does

| Layer | What Pocket Mode is |
|---|---|
| `crates/layout` | `LayoutOptions::mobile`: indents and default tab stops in proportion to the screen, list labels closer to their text, picture-only line heights, and drawings text wraps round placed inline |
| `crates/engine` | `view.pocketMode` (`{"value"?: bool}`) and `view.pocketWidth` (`{"value": points}`, 72–4000, default 360), with `mobile`/`pocket_width` kept in the view state |
| `crates/ui-egui` | the toggle in View › Views, `pocketMode`/`pocketWidth` in `ui.json`, and the hand-off call |
| `crates/android-bridge` | the UniFFI bridge the Compose app loads; `set_view("mobile", width)` drives the same commands |
| `apps/wordcraft-android-compose` | the phone front end, packaged in the same APK as the egui one |

The layout mode is **inert unless set**: with `mobile: false` the layout is byte-identical to a
build without the feature, which `mobile_false_lays_out_exactly_as_today` in
`crates/layout/src/tests.rs` pins.

## The pocket column is not `web_width`

Web Layout wraps to the work area, and `crates/ui-egui/src/canvas.rs` rewrites `view.web_width`
from the window's size every frame it draws (#1876). Pocket Mode's column is a **fixed** phone
width (`view.pocket_width`), so it is a separate field: reusing `web_width` would make the pocket
column track the window and never hold a phone's width. The canvas skips its `web_width` write
while pocket mode is on, so the two cannot fight.

## The switch on Android is a hand-off, not a live swap

The two front ends are different activity kinds — the egui one is a `NativeActivity` whose
`android_main` hands the window to eframe/winit, the Compose one is a `ComponentActivity` with
`setContent` — and **each owns its window and its loop**. Android cannot swap one activity's UI for
another's in place, so switching is a hand-off: the running front end makes the document safe,
starts the other activity, and that one re-opens the document.

```
View › Views › Pocket Mode
  → view.pocketMode {value:true}
  → the app saves the document (or writes the recovery copy when it has never been saved)
  → startActivity(the other front end's activity, FLAG_ACTIVITY_CLEAR_TOP)
  → the other front end re-opens the document and shows the pocket view
```

The document is made safe **before** anything is started, and a failed save **cancels** the switch
with a status rather than proceeding: an edit lost to a half-finished switch is worse than a switch
that did not happen.

### Both front ends in one APK

`cargo-apk` — which the egui Android port uses — supports exactly one `<activity>` (its metadata
struct is `activity: Activity`, defaulting to `android.app.NativeActivity`), so it cannot declare a
second one. The APK is therefore packaged by the **Gradle** project
(`apps/wordcraft-android-compose/android`), which declares both activities and takes both Rust
libraries as generated jniLibs:

| Activity | Class | Library | Role |
|---|---|---|---|
| Compose | `io.github.artcraftandroid.wordcraft.MainActivity` | `libwordcraft_android_bridge.so` | the launcher, the pocket front end |
| egui | `android.app.NativeActivity` | `libwordcraft_android.so` | the desktop-style front end |

The hand-off targets these explicitly (`setClassName`), so nothing outside the APK can be reached.

## On the desktop and the web

There is only one front end, so the switch cannot hand off — `Services::pocket_handoff` is `None`
and the app applies the pocket presentation alone (the fixed phone-width column). This is what
keeps the feature testable headlessly, without a device.

## Testing

- `crates/layout/src/tests.rs`: the mobile layout, and that `mobile: false` changes nothing.
- `crates/engine/src/tests.rs`: `pocket_mode_toggles_web_layout_and_back`,
  `pocket_width_clamps_and_never_stores_a_non_finite_width`,
  `pocket_state_survives_a_restart_and_clamps_junk`.
- `crates/ui-egui/src/ribbon.rs`: the toggle is on the View tab and runs `view.pocketMode`.
- `crates/ui-egui/src/lib.rs`: the `ui.json` round trip with junk values.
- `crates/android-bridge/src/tests.rs`: `mobile_view_reflows_to_the_width_and_print_brings_the_pages_back`
  and the rest of the ported suite (52 tests).
