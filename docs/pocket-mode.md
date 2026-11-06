# Pocket Mode

Pocket Mode is WordCraft's phone/tablet presentation: the document reflowed to a pocket's column
instead of laid out as pages. It is a first-class view state (`view.pocketMode`), a switch in
**View › Views** on the egui front end, a switch in **Settings › Display Preferences** on the phone
front end, and — on Android — the way the two front ends hand the session to each other.

*Status: the engine, the layout mode, the switch and the bridge are implemented and tested. On the phone the
interface options sit in Settings › Display Preferences, and Desktop Mode is pinned to landscape
(2026-11-06: checked against the packaged manifest by CI, and re-run on a phone — the run also found
that the pocket column overflows the screen at its default width, below). **The desktop interface is
now remembered**: WordCraft reopens on it and the back gesture leaves the app instead of uncovering
Pocket Mode, which stays the way back. Both hand-off
directions now run on a phone with the CI-built debug APK: phone → desktop, and Pocket Mode → phone,
which previously posted nothing because JNI's class lookup could not see the app's own classes from
egui's native thread. Returning to the desktop front end *after* Pocket Mode is a separate open limit
(egui's event loop cannot be recreated in that process) — see "What is verified, and what is not".*

## What it does

| Layer | What Pocket Mode is |
|---|---|
| `crates/layout` | `LayoutOptions::mobile`: indents and default tab stops in proportion to the screen, list labels closer to their text, picture-only line heights, and drawings text wraps round placed inline |
| `crates/engine` | `view.pocketMode` (`{"value"?: bool}`) and `view.pocketWidth` (`{"value": points}`, 72–4000, default 360), with `mobile`/`pocket_width` kept in the view state |
| `crates/ui-egui` | the toggle in View › Views, `pocketMode`/`pocketWidth` in `ui.json`, and the hand-off call |
| `crates/android-bridge` | the UniFFI bridge the Compose app loads; `set_view("mobile", width)` drives the same commands |
| `apps/wordcraft-android-compose` | the phone front end, packaged in the same APK as the egui one; its **Settings › Display Preferences** holds Pocket Mode and Desktop Mode |

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
demand through **Settings › Display Preferences › Desktop Mode** (the ⋯ › Open Desktop Interface row
until 2026-11-06, over the same request channel), since the desktop activity is not exported and nothing
outside the APK — not even adb — can start it.

The document is made safe **before** anything is started, and a failed save **cancels** the switch
with a status rather than proceeding: an edit lost to a half-finished switch is worse than a switch
that did not happen.

## The two interface options live in Settings › Display Preferences

The phone app's Settings carries both interface options under **Display Preferences**, in order:
**Theme**, **Pocket Mode** (documents open reflowed to the phone's column rather than as pages — it
kept its stored `startMobileView` key, so a choice made before the move still reads back), and
**Desktop Mode**, which opens this APK's desktop-style front end. General now holds only AutoSave,
and the ⋯ sheet no longer offers the desktop front end: the option **moved**, it is not duplicated.

Desktop Mode is **landscape only**. The desktop activity it opens (`DesktopActivity`, a `NativeActivity` subclass) declares
`android:screenOrientation="landscape"` in the Gradle manifest, and `apps/wordcraft-android` pins
the same for the `cargo-apk` build (`orientation = "landscape"`), where that activity is the only
one in the APK. Turning the phone portrait therefore never re-lays the desktop window out. The
Compose activity keeps the sensor, so Pocket Mode still reads whichever way the phone is held. The
Android workflow reads the **packaged** manifest and fails if either declaration is wrong.

Two switches are not the same thing, and neither is hidden by this move: the desktop front end's
own Pocket Mode switch stays in **View › Views** (Word parity on every front end with a ribbon, and
the desktop app's File › Options › Display pane is unchanged), while the phone editor's bar button
toggles Mobile View *inside* the phone front end — a presentation, not a front-end hand-off.

### Both front ends in one APK

`cargo-apk` — which the egui Android port uses — supports exactly one `<activity>` (its metadata
struct is `activity: Activity`, defaulting to `android.app.NativeActivity`), so it cannot declare a
second one. The APK is therefore packaged by the **Gradle** project
(`apps/wordcraft-android-compose/android`), which declares both activities and takes both Rust
libraries as generated jniLibs:

| Activity | Class | Library | Role |
|---|---|---|---|
| Compose | `io.github.artcraftandroid.wordcraft.MainActivity` | `libwordcraft_android_bridge.so` | the launcher, the pocket front end |
| egui | `DesktopActivity` (a `NativeActivity` subclass) | `libwordcraft_android.so` | the desktop-style front end, pinned to landscape |

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
… result code=0` from ⋯ › Open Desktop Interface (that row is now Settings › Display Preferences ›
Desktop Mode, the same request).

****The other direction had not switched.** Tapping View › Views › Pocket Mode raised the soft keyboard and
left egui on screen; the app's own log recorded `android access: could not ask the phone app to switch
front ends` five times in one minute, and no `startActivity` for `MainActivity` ever appeared. The cause
was the class lookup, not the request: JNI's `FindClass` resolves through the *current thread's* loader,
and `android_main` runs on a native thread android-activity spawns with no Java frame on the stack, so
that loader is Android's system loader, which never holds this APK's classes. Framework classes
(`android/os/Environment`, `android/content/Intent`) are in the system loader, which is why the storage
grant worked by name; the phone app's Kotlin object is not. It is now loaded through the loader read off
the `Application` object — a lookup that works on any thread — and a failed hand-off logs the Java
exception that says why (`apps/wordcraft-android/src/android_access.rs`).

**The fixed hand-off is verified on that phone (2026-11-06, same device, run 42154378675's debug APK).**
The CI debug keystore differs from the debug build already installed, so the install replaces it
(`adb uninstall` first); app-private files and the two shared documents were copied off beforehand and
restored afterwards. egui was started with `pocketMode: true` restored from `ui.json`, which dispatches
the same `view.pocketMode` interception the ribbon switch uses (`crates/ui-egui/src/lib.rs:1132`), and
logcat recorded `START … cmp=…/io.github.artcraftandroid.wordcraft.MainActivity … flg=0x20000
(REORDER_TO_FRONT) … from uid 10292 (io.github.artcraftandroid.wordcraft.debug) … result code=3`:
`MainActivity` became the focused window and the screen showed the phone editor. The app's log carried
no `could not ask the phone app to switch front ends` line, which is what five taps recorded before the
fix. The negative control holds too: with `pocketMode: false`, `NativeActivity` keeps the screen.

The switch was driven through saved state, not the button: `adb`'s blind touch injection does not reach
egui's scaled hit targets reliably (`uiScale` 175, and the soft keyboard resizes the window), so View ›
Views › Pocket Mode could not be tapped from the shell. The saved-state path runs the identical
interception and the identical JNI hand-off, so it proves the fix; tapping the ribbon by hand remains
the unconfirmed part.

**The Settings move and the landscape pin are verified on the same phone (2026-11-06, POCO X6 Pro,
Android 17/API 37, run 42182262170's debug APK, installed over the local debug build — the CI debug
keystore differs, so `adb uninstall` first, which clears app-private storage).** Settings showed
Display Preferences › Theme, **Pocket Mode** ("Reflow documents to a phone's column instead of pages")
and **Desktop Mode** ("Opens the desktop interface: landscape only, portrait is not available"), with
General holding only AutoSave, and the ⋯ sheet no longer listed the desktop front end. Tapping Desktop
Mode put `NativeActivity` on screen with the window measured **2712x1220** while the phone was held
portrait with auto-rotate on, and the window stayed landscape when Android was forced to portrait
(`user_rotation=0`, auto-rotate off) and then to landscape — Desktop Mode never shows a portrait
screen, whatever the phone is doing. Back from it returned `MainActivity`, whose window measured
1220x2712 forced portrait and 2712x1220 forced landscape: Pocket Mode still turns with the device.
Turning Pocket Mode off and restarting the app left it off, stored under the retained `startMobileView`
key.

**Desktop Mode persists (2026-11-06).** The choice is stored (`desktopInterface`, in the phone app's
`settings` preferences): Settings › Display Preferences › Desktop Mode sets it, and egui's View ›
Views › Pocket Mode clears it as it hands back. Opening WordCraft on a remembered desktop interface
redirects the launcher activity to `DesktopActivity` — which is not exported, so only an in-app intent
can reach it — and the phone activity retires when egui reports its window is up, which is why the
back gesture no longer uncovered Pocket Mode "like another page". The report is posted over JNI from
the app creator (`apps/wordcraft/src/lib.rs`), which eframe runs only once its event loop and window
exist: a desktop front end that cannot recreate its loop never reports, so the phone interface stays
on screen rather than an empty task. Both hand-off channels are drained by `WordcraftApplication` now,
not by the phone activity, because that activity may be finished while egui is on screen.

**The desktop front end hides the phone's bars (2026-11-06).** egui's ribbon tabs live at the very top
of its window, and Android's status bar drew over them and took the touches: a finger could never reach
View › Views › Pocket Mode, which is the only way back once Desktop Mode is remembered. So the desktop
activity is now a `NativeActivity` subclass, `DesktopActivity`, that hides the system bars and brings them
back on a swipe from an edge (`WindowInsetsController`, API 30+; on API 29 the bars stay as Android draws
them, which is the old behaviour).
**Auto means 100 % on Android (`crates/ui-egui/src/theme.rs`).** Android reports the screen's density
as the display scale — 3.0 on a 480 dpi phone — and Auto followed it, drawing the whole interface
three times as large. On that build Auto is one screen pixel per point; desktop and web keep following
the system's scale, and a size chosen in File › Options › Display stays absolute everywhere.
**What that run found:** at the default pocket width the phone's pocket column is **wider than the
screen** — text is cut off at the right edge and a horizontal swipe is needed to read the rest, while
Print View wraps inside the viewport. Nothing in this arc touched the layout, so it is Pocket Mode's
own open follow-up on a phone: the column should fit the screen at its default width rather than
overflow it.

**Still failing, and separate:** Pocket Mode destroys egui's activity once the phone front end takes the
screen, so Settings › Display Preferences › Desktop Mode afterwards starts `DesktopActivity` but egui
exits at once —
`wordcraft exited with an error: winit EventLoopError: EventLoop can't be recreated` — and the phone
editor stays. Returning *from* Pocket Mode in the same process needs its own fix: keep egui alive while
pocket mode is on, or let the entry crate build a fresh event loop for a second `android_main`.
`REORDER_TO_FRONT` (commit `e5195f65`) exists because a destroyed `NativeActivity` cannot be relaunched,
so this is the same limitation seen from the other side.

Persistence does not hide this: with the choice remembered, each launch redirects to `DesktopActivity`,
which fails, and the phone interface stays on screen each time — the choice only clears when Pocket
Mode hands back from a desktop front end that really came up.

## Testing

- `crates/layout/src/tests.rs`: the mobile layout, and that `mobile: false` changes nothing.
- `crates/engine/src/tests.rs`: `pocket_mode_toggles_web_layout_and_back`,
  `pocket_width_clamps_and_never_stores_a_non_finite_width`,
  `pocket_state_survives_a_restart_and_clamps_junk`.
- `crates/ui-egui/src/ribbon.rs`: the toggle is on the View tab and runs `view.pocketMode`.
- `crates/ui-egui/src/lib.rs`: the `ui.json` round trip with junk values.
- `crates/android-bridge/src/tests.rs`: `mobile_view_reflows_to_the_width_and_print_brings_the_pages_back`
  and the rest of the ported suite (52 tests).
- `.github/workflows/android.yml`: the packaged manifest pins `DesktopActivity` to landscape (Desktop
  Mode) and leaves `MainActivity` on the sensor (Pocket Mode turns with the device), read by **activity
  block** — `aapt2 dump xmltree` writes enum attributes as numbers, so landscape is `0`, portrait `1`
  and an undeclared orientation is no line at all; grepping the dump for the word "landscape" fails on
  a correctly pinned APK (run 42176542755 did exactly that), and the number is read from a fixture the
  workflow builds with the runner's own framework table rather than hardcoded.
- `apps/wordcraft-android/src/lib.rs`: `pocket_handoff_loads_the_class_with_the_apps_own_loader` pins the
  hand-off's class lookup, since JNI itself only runs on a device.
