//! Android entry point for WordCraft.
//!
//! Android's `NativeActivity` loads this library and calls [`android_main`] on a dedicated
//! thread. It opens the shared app ([`wordcraft::run_android`], the same code the desktop binary
//! runs) and returns when the window closes, ending the activity's main-loop thread.
//!
//! This crate exists because that export must be unmangled, and on edition 2024 the only spelling
//! (`#[unsafe(no_mangle)]`) trips the workspace-forbidden `unsafe_code` lint — see `Cargo.toml`
//! for why the crate carries its own lints instead. It also holds `android_access` (Android
//! builds), this crate's one piece of real `unsafe` — the JNI to the activity's JVM that checks
//! and opens Android's "All files access" grant, named as the exception in AGENTS.md (Never
//! crash) — whose safe closures are handed to the shared app as [`wordcraft::SharedStorageAccess`].
//!
//! Android limitations (v1): no system file picker and no system message dialogs (rfd has no
//! Android backend), so Save, Open, exports and picture import are served by WordCraft's own
//! built-in file browser: app-private storage always, shared storage (`/storage/emulated/0`)
//! too once the user grants "All files access" (`android_access`); and there is no per-user
//! settings folder (`HOME` and `XDG_CONFIG_HOME` are unset): preferences, logs, recovery, user
//! fonts and templates live in the app-private storage directory the activity resolves.
//!
//! On every other target this crate is empty: only Android's loader ever looks for `android_main`.

#[cfg(target_os = "android")]
mod android_access;

/// NativeActivity's main-loop thread entry point (required; see the `android-activity` docs).
/// winit pumps the activity's lifecycle and input events inside [`wordcraft::run_android`], which
/// needs the activity's handle to create its event loop; when the activity is destroyed that
/// returns and this returns with it.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: android_activity::AndroidApp) {
    // The "All files access" grant, checked and requested over JNI — this crate's unsafe stays
    // in `android_access`; `run_android` only ever sees the plain closures built here.
    let shared = android_access::capture(&app).map(|a| wordcraft::SharedStorageAccess {
        granted: std::sync::Arc::new(move || a.shared_storage_granted()),
        ask: std::sync::Arc::new(move || a.ask_shared_storage_grant()),
        // Pocket Mode's hand-off: start the other front end's activity, which re-opens the
        // document this one has just made safe.
        switch_front_end: Some(std::sync::Arc::new(move |pocket: bool| a.start_other_front_end(pocket))),
        // "This window is up", reported from the app creator: the phone app finishes its own
        // activity on it, so the desktop interface is the only one left in the task and the back
        // gesture leaves WordCraft rather than uncovering the pocket screen underneath.
        front_end_up: Some(std::sync::Arc::new(move || a.report_interface_up())),
    });
    if let Err(e) = wordcraft::run_android(app, shared) {
        log::error!("wordcraft exited with an error: {e}");
    }
}

/// Pocket Mode's hand-off must reach the phone app's Kotlin object through the loader that actually
/// holds this APK's classes. Asking for a JNI call by class *name* makes JNI run `FindClass`, which
/// resolves through the *current thread's* loader; `android_main` runs on a native thread that
/// android-activity spawns from native code, with no Java frame on its stack, so that loader is
/// Android's system loader and never holds this APK's classes. Asked by name the request posted
/// nothing: the phone logged `could not ask the phone app to switch front ends` five times while
/// Pocket Mode did nothing but raise the keyboard (2026-11-06). JNI only runs on a device, so this
/// pins the source instead, the way `third_party/winit/tests/ime_patches.rs` pins the vendored
/// patches.
#[test]
fn pocket_handoff_loads_the_class_with_the_apps_own_loader() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/android_access.rs");
    let src = std::fs::read_to_string(&path).unwrap();
    // The hand-off entry point is the last function in the file, so this is its body and helpers.
    let hand_off = src.split("pub fn start_other_front_end").nth(1).unwrap();
    assert!(hand_off.contains("getApplication"), "the loader is read off the running app");
    assert!(hand_off.contains("getClassLoader"), "the app's own loader must be asked for");
    assert!(hand_off.contains("loadClass"), "and the class loaded through it");
    assert!(hand_off.contains("call_static_method(&handoff"), "the request goes to the loaded class, not a name");
    assert!(hand_off.contains("requestPocket"), "the phone app's hand-off object is still the target");
    assert!(
        !hand_off.contains("io/github/artcraftandroid/wordcraft/FrontEndHandoff"),
        "the hand-off must not ask JNI's FindClass for a class of this APK: this thread's loader cannot see it"
    );
}

/// The phone app retires its own activity only after egui's window really exists, so a desktop
/// front end that cannot recreate its event loop leaves the phone interface on screen instead of an
/// empty task. JNI only runs on a device, so this pins the source: the report is posted through the
/// app's own loader — the same reason `start_other_front_end` must be — and the shared app carries
/// the hook and calls it from the app creator, which eframe runs only once the loop and window are
/// built.
#[test]
fn interface_up_report_posts_through_the_apps_own_loader() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let access = std::fs::read_to_string(dir.join("src/android_access.rs")).unwrap();
    let post = access.split("fn post_interface_up").nth(1).unwrap();
    assert!(post.contains("getApplication"), "the loader is read off the running app");
    assert!(post.contains("getClassLoader"), "the app's own loader must be asked for");
    assert!(post.contains("loadClass"), "and the class loaded through it");
    assert!(post.contains("reportInterfaceUp"), "the phone app's hand-off object gets the report");
    assert!(
        !post.contains("io/github/artcraftandroid/wordcraft/FrontEndHandoff"),
        "the report must not ask JNI's FindClass for a class of this APK: this thread's loader cannot see it"
    );
    assert!(access.contains("pub fn report_interface_up"), "the entry crate exposes the report");

    let shared = std::fs::read_to_string(dir.join("../wordcraft/src/lib.rs")).unwrap();
    assert!(shared.contains("pub front_end_up: Option<"), "the shared app carries the report hook");
    assert!(shared.contains("access.front_end_up"), "and calls it once the window exists");
}
