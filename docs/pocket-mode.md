# Pocket Mode

Pocket Mode is WordCraft's phone/tablet presentation: the document reflowed to a pocket's column
instead of laid out as pages. It is a first-class view state (`view.pocketMode`), a switch in
**View › Views**, and — on Android — the way the two front ends hand the session to each other.

*Status: the engine, the layout mode, the switch and the bridge are implemented and tested; the
Android hand-off runs on a phone, and the phone → desktop direction is verified there. Pocket Mode did
not switch on the phone — JNI's class lookup could not see the app's own classes — and that is fixed;
the fixed direction needs the rebuilt APK to confirm on device.*

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
  → egui's native thread posts the request to the phone app over JNI: load
    io.github.artcraftandroid.wordcraft.FrontEndHandoff with **the app's own class loader**
    (read off the Application object), then call its static requestPocket(pocket)
  → the phone app drains it on its main thread: startActivity(the other front end's activity,
    FLAG_ACTIVITY_REORDER_TO_FRONT)
  → the other front end re-opens the document and shows the pocket view
```

The request is posted rather than started directly because egui runs on a **native thread**, and
Android 14+ can silently drop an activity started from a background thread — the launch has to
happen on the activity's main thread, which is the phone app's. Coming back out of Pocket Mode
posts the same request with the egui activity as the target; the phone app also reaches egui on
demand through **⋯ › Open Desktop Interface**, since `NativeActivity` is not exported and nothing
outside the APK — not even adb — can start it.

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

The phone app's main-thread loop targets these explicitly (`setClassName` with the running
package, so the debug build's `applicationIdSuffix` resolves correctly), reaching nothing outside
the APK.

## On the desktop and the web

There is only one front end, so the switch cannot hand off — `Services::pocket_handoff` is `None`
and the app applies the pocket presentation alone (the fixed phone-width column). This is what
keeps the feature testable headlessly, without a device.

## What is verified, and what is not

Verified (2026-11-05): the layout mode (5 tests, including that `mobile: false` changes nothing),
the engine commands and their persistence, the View › Views toggle and the `ui.json` round trip,
the 52-test bridge suite, `cargo xtask layers` (23 crates) and `cargo xtask assets`; and that **both**
Android libraries cross-compile for arm64 (`libwordcraft_android_bridge.so` and
`libwordcraft_android.so`, the latter with `android_main` exported).

Verified on a phone (2026-11-06, POCO X6 Pro, Android 17/API 37, the CI debug APK): the app runs, and
the **phone → desktop** hand-off fires — `ActivityTaskManager` logged
`START … cmp=io.github.artcraftandroid.wordcraft.debug/android.app.NativeActivity … flg=REORDER_TO_FRONT
… result code=0` from ⋯ › Open Desktop Interface.

**The other direction did not switch.** Tapping View › Views › Pocket Mode raised the soft keyboard and
left egui on screen; the app's own log recorded `android access: could not ask the phone app to switch
front ends` five times in one minute, and no `startActivity` for `MainActivity` ever appeared. The cause
was the class lookup, not the request: JNI's `FindClass` resolves through the *current thread's* loader,
and `android_main` runs on a native thread android-activity spawns with no Java frame on the stack, so
that loader is Android's system loader, which never holds this APK's classes. Framework classes
(`android/os/Environment`, `android/content/Intent`) are in the system loader, which is why the storage
grant worked by name; the phone app's Kotlin object is not. It is now loaded through the loader read off
the `Application` object — a lookup that works on any thread — and a failed hand-off logs the Java
exception that says why (`apps/wordcraft-android/src/android_access.rs`).

Not verified yet: **the fixed hand-off on a device.** This machine cannot build the APK — it has NDK 29
and 30 but no JDK 17–21, so `assembleDebug` cannot run here; the APK comes from GitHub Actions. Rebuilding
it, installing it, and tapping Pocket Mode is the remaining verification. Installing a CI debug APK over
the debug build already on the phone needs `adb uninstall` first (the debug keystores differ), which wipes
both front ends' app-private storage, so it is the owner's call. `cargo xtask ci` and the pinned source
check (`apps/wordcraft-android/src/lib.rs`) do cover the change itself.

## Testing

- `crates/layout/src/tests.rs`: the mobile layout, and that `mobile: false` changes nothing.
- `crates/engine/src/tests.rs`: `pocket_mode_toggles_web_layout_and_back`,
  `pocket_width_clamps_and_never_stores_a_non_finite_width`,
  `pocket_state_survives_a_restart_and_clamps_junk`.
- `crates/ui-egui/src/ribbon.rs`: the toggle is on the View tab and runs `view.pocketMode`.
- `crates/ui-egui/src/lib.rs`: the `ui.json` round trip with junk values.
- `crates/android-bridge/src/tests.rs`: `mobile_view_reflows_to_the_width_and_print_brings_the_pages_back`
  and the rest of the ported suite (52 tests).
- `apps/wordcraft-android/src/lib.rs`: `pocket_handoff_loads_the_class_with_the_apps_own_loader` pins the
  hand-off's class lookup, since JNI itself only runs on a device.
