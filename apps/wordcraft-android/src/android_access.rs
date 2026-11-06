//! Android's "All files access" grant (`MANAGE_EXTERNAL_STORAGE`), reached over JNI.
//!
//! The in-app file browser (`wordcraft_ui_egui::file_browser`) would otherwise only ever see the
//! app-private directory. To let a save or export reach shared storage (`/storage/emulated/0`),
//! the user must grant "All files access" — a special permission Android never grants silently:
//! the app can only *check* whether it is held and *open the screen* where the user toggles it
//! (Android 11+), or show the ordinary runtime permission dialog (Android 9, whose legacy
//! `WRITE_EXTERNAL_STORAGE` still opens the whole shared tree). Android 10 targets scoped
//! storage, where no grant opens the shared tree, so nothing is offered there. That is all this
//! module does — it never reads or writes storage itself, so the browser's own root-clamping
//! still keeps every answer inside the root the user is browsing.
//!
//! Every JNI call can fail (no JVM, an API level without the method, a device that refuses the
//! intent) and every failure degrades to "not granted" / "nothing happened", logged at worst —
//! never a panic, which this crate's lints deny too.
//!
//! This is the entry crate's one piece of real `unsafe`: `jni`'s entry points to the activity's
//! raw JVM (`JavaVM::from_raw`, `JObject::from_raw`) have no safe form, which is why AGENTS.md
//! (Never crash) names this file as the workspace's `unsafe` exception and confines it here: the
//! module holds the pointers, never lets them leave, and hands [`wordcraft::SharedStorageAccess`]
//! plain closures, so the shared app crates stay unsafe-free.

use jni::objects::{JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

/// The pocket front end's activity class, as the Gradle manifest declares it. The debug build's
/// `applicationIdSuffix` changes the package, never the class name, so the hand-off resolves it
/// inside whatever package the running app has.
const COMPOSE_ACTIVITY: &str = "io.github.artcraftandroid.wordcraft.MainActivity";
/// This front end's activity, as the manifest declares it (`android:name="android.app.NativeActivity"`).
const EGUI_ACTIVITY: &str = "android.app.NativeActivity";
/// The phone app's hand-off object, as a **binary** class name — the spelling `ClassLoader.loadClass`
/// takes. JNI's own name form (`io/github/...`) is the one `FindClass` wants, and that lookup never
/// finds this class here: see `start_other_front_end`.
const COMPOSE_HANDOFF: &str = "io.github.artcraftandroid.wordcraft.FrontEndHandoff";

/// The activity's JVM and activity reference, captured before the `AndroidApp` moves into
/// eframe. The activity owns both and lives as long as the app, so every later call is valid.
#[derive(Clone, Copy)]
pub struct AndroidAccess {
    vm: *mut jni::sys::JavaVM,
    activity: jni::sys::jobject,
}

// SAFETY: both pointers belong to the activity and are never freed while the app runs, which is
// the only time they are touched (on the single thread that runs the UI); jni's `from_raw`
// adopts them without owning them, so sharing them between the closures handed to the app is
// sound.
unsafe impl Send for AndroidAccess {}
unsafe impl Sync for AndroidAccess {}

/// Capture the handles from the activity. `None` (and a log) when no JVM is attached, in which
/// case the browser simply keeps its app-private root.
pub fn capture(app: &android_activity::AndroidApp) -> Option<AndroidAccess> {
    let vm = app.vm_as_ptr() as *mut jni::sys::JavaVM;
    let activity = app.activity_as_ptr() as jni::sys::jobject;
    if vm.is_null() || activity.is_null() {
        log::warn!("android access: no JVM on the activity; \"All files access\" stays unavailable");
        return None;
    }
    Some(AndroidAccess { vm, activity })
}

impl AndroidAccess {
    /// Run one JNI session on this thread: attach, hand `f` the environment, then make sure no
    /// Java exception is left pending (the JVM would rethrow it into unrelated code later —
    /// checking and clearing is allowed while an exception is pending). `None` when the JVM
    /// could not be reached or `f` erred.
    fn with_env<T>(self, f: impl FnOnce(&mut JNIEnv) -> jni::errors::Result<T>) -> Option<T> {
        // SAFETY: `vm` was checked non-null in `capture` and belongs to the activity (alive for
        // the app's whole lifetime); `from_raw` adopts the raw pointer without owning or
        // freeing it, and the attach guard scopes this thread's attachment to the call.
        let vm = unsafe { JavaVM::from_raw(self.vm) }.ok()?;
        let mut guard = vm.attach_current_thread().ok()?;
        let out = f(&mut *guard).ok();
        if guard.exception_check().unwrap_or(false) {
            let _ = guard.exception_clear();
        }
        out
    }

    /// The activity object for one call: its own (never freed) reference, adopted by
    /// `from_raw` without owning or deleting it.
    fn activity<'local>(self) -> JObject<'local> {
        // SAFETY: the raw pointer is the activity's own reference, valid while the app is; the
        // value is used for this one call only.
        unsafe { JObject::from_raw(self.activity) }
    }

    /// `Ok(true)` when the preceding Java call did not throw; a throw shows as a pending
    /// exception (which is cleared here, since most JNI calls are illegal while one is set).
    fn started(env: &mut JNIEnv) -> jni::errors::Result<bool> {
        if env.exception_check()? {
            env.exception_clear()?;
            return Ok(false);
        }
        Ok(true)
    }

    /// The running app's package (`Context.getPackageName()`): the release app's package, or the
    /// debug build's with its `applicationIdSuffix`. The grant URI and the hand-off intent must
    /// name *this* package, so it is read from the activity rather than assumed.
    fn runtime_package(env: &mut JNIEnv, act: &JObject) -> jni::errors::Result<String> {
        let value = env.call_method(act, "getPackageName", "()Ljava/lang/String;", &[])?;
        let text: jni::objects::JString = value.l()?.into();
        Ok(env.get_string(&text)?.into())
    }

    /// The API level (`Build.VERSION.SDK_INT`), `None` when the JVM could not answer.
    fn sdk(self) -> Option<i32> {
        self.with_env(|env| env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i())
    }

    /// Whether shared storage may be read and written right now: "All files access" on
    /// Android 11+, the legacy storage permission on Android 9. `false` on Android 10 (scoped
    /// storage, no "All files access" page yet) and when unknown: the browser then keeps its
    /// app-private root.
    pub fn shared_storage_granted(self) -> bool {
        match self.sdk() {
            // `Environment.isExternalStorageManager()` exists only on API 30+.
            Some(sdk) if sdk >= 30 => {
                self.with_env(|env| env.call_static_method("android/os/Environment", "isExternalStorageManager", "()Z", &[])?.z()).unwrap_or(false)
            }
            // Android 9: the legacy runtime permission, which still unlocks the whole tree.
            // Android 10 is not offered: this app targets 35, so scoped storage applies there and
            // direct file paths to shared storage are unavailable (`requestLegacyExternalStorage`
            // is ignored for such targets, and cargo-apk cannot declare it anyway).
            Some(28) => self
                .with_env(|env| -> jni::errors::Result<bool> {
                    let perm = env.new_string("android.permission.WRITE_EXTERNAL_STORAGE")?;
                    let act = self.activity();
                    let code = env.call_method(&act, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?.i()?;
                    Ok(code == 0)
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    /// Ask the user to grant it: Android 11+ opens this app's "All files access" settings page,
    /// Android 9 shows the runtime permission dialog (the browser re-checks the grant on every
    /// visit, so the answer needs no callback). Nothing is askable on Android 10, where no grant
    /// opens shared storage. Best effort: a device that refuses the intent or an unreachable JVM
    /// just leaves the grant un-granted, logged.
    pub fn ask_shared_storage_grant(self) {
        let asked = match self.sdk() {
            Some(sdk) if sdk >= 30 => self.with_env(|env| -> jni::errors::Result<bool> {
                // uri = Uri.parse("package:<the running app's package>")
                let act = self.activity();
                let uri_text = format!("package:{}", Self::runtime_package(env, &act)?);
                let pkg = env.new_string(uri_text.as_str())?;
                let uri =
                    env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&pkg)])?.l()?;
                // intent = new Intent(ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, uri)
                let action: JObject = env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?.into();
                let intent = env.new_object(
                    "android/content/Intent",
                    "(Ljava/lang/String;Landroid/net/Uri;)V",
                    &[JValue::Object(&action), JValue::Object(&uri)],
                )?;
                // activity.startActivity(intent)
                env.call_method(&act, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
                Self::started(env)
            }),
            Some(28) => self.with_env(|env| -> jni::errors::Result<bool> {
                // The permission dialog: requestPermissions(new String[]{read, write}, 42).
                let arr = env.new_object_array(2, "java/lang/String", &JObject::null())?;
                let perms = ["android.permission.READ_EXTERNAL_STORAGE", "android.permission.WRITE_EXTERNAL_STORAGE"];
                for (i, perm) in perms.into_iter().enumerate() {
                    let s = env.new_string(perm)?;
                    env.set_object_array_element(&arr, i as jni::sys::jsize, &s)?;
                }
                let act = self.activity();
                env.call_method(&act, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&arr), JValue::Int(42)])?;
                Self::started(env)
            }),
            _ => None,
        };
        if asked != Some(true) {
            log::warn!("android access: could not ask for shared-storage access (the browser keeps its app-private root)");
        }
    }

    /// Hand the session to the other front end. `pocket` picks the direction — the Compose
    /// app's `MainActivity` for the pocket front end, this app's own `NativeActivity` to come
    /// back.
    ///
    /// Android has no way to swap one activity's UI for another's in place (each owns its window
    /// and its loop), so the switch is a hand-off: the caller has already made the document safe,
    /// and the activity this call starts re-opens it.
    ///
    /// This call runs on egui's native thread, where Android (14+) can silently drop an activity
    /// started from a background thread, so the request is posted to the phone app's Kotlin object
    /// (`FrontEndHandoff.requestPocket`) and its main-thread loop performs the `startActivity` —
    /// with an explicit in-app intent and `FLAG_ACTIVITY_REORDER_TO_FRONT`, which brings the other
    /// front end back rather than destroying it (a destroyed `NativeActivity` cannot be relaunched),
    /// and reaches nothing outside this APK.
    ///
    /// The object is loaded through **the app's own class loader**, never asked for by name. JNI's
    /// `FindClass` — what `call_static_method` does when handed a class *name* — resolves through
    /// the current thread's loader, and with no Java frame on the stack that is Android's *system*
    /// loader, which never holds this APK's classes. `android_main` runs on exactly such a thread:
    /// android-activity spawns it from native code, and its own glue says what that costs
    /// (`native_activity/glue.rs`: "difficult for native code to look up non-standard Java
    /// classes"). Framework classes live in the system loader, so the storage grant above is fine
    /// asked by name; the phone app's Kotlin object does not, and asking for it that way posted
    /// nothing — the phone's log recorded `could not ask the phone app to switch front ends` five
    /// times while Pocket Mode did nothing but raise the keyboard. Reading the loader off the
    /// `Application` object, as `runtime_package` reads the package, works on any thread, so the
    /// request lands.
    ///
    /// Best effort, like the storage grant: an unreachable JVM, a class that won't load or a method
    /// that isn't there logs what failed and returns `false`, and the caller reports that the switch
    /// did not happen rather than pretending it did.
    pub fn start_other_front_end(self, pocket: bool) -> bool {
        let target = if pocket { COMPOSE_ACTIVITY } else { EGUI_ACTIVITY };
        let posted = self.with_env(|env| -> jni::errors::Result<(bool, String)> {
            let act = self.activity();
            match post_handoff(env, &act, pocket) {
                // `started` reports whether Java left anything pending: a pending exception here
                // means the post did not land, and it is cleared before anything else is called.
                Ok(()) => Ok(match Self::started(env) {
                    Ok(true) => (true, String::new()),
                    Ok(false) => (false, "the phone app refused the request".into()),
                    Err(why) => (false, format!("checking the request: {why}")),
                }),
                Err(why) => Ok((false, failure_note(env, &why))),
            }
        });
        match posted {
            Some((true, _)) => true,
            Some((false, why)) => {
                log::warn!("android access: could not ask the phone app to switch front ends ({why}; {target} stayed behind)");
                false
            }
            None => {
                log::warn!("android access: no JVM on the activity; {target} stayed behind");
                false
            }
        }
    }
}

/// Post Pocket Mode's switch request to the phone app's Kotlin object.
///
/// `ClassLoader.loadClass` is called on the loader read off the `Application` object, because that
/// loader is the one holding this APK's classes; see `AndroidAccess::start_other_front_end` for why
/// asking JNI by class name cannot find them from this thread. Every step can fail, and the error is
/// returned for the caller to log — never a panic, which this module's lints deny too.
fn post_handoff(env: &mut JNIEnv, act: &JObject, pocket: bool) -> jni::errors::Result<()> {
    let app = env.call_method(act, "getApplication", "()Landroid/app/Application;", &[])?.l()?;
    let app_class = env.get_object_class(&app)?;
    let loader = env.call_method(&app_class, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
    let handoff_name = env.new_string(COMPOSE_HANDOFF)?;
    let handoff: JClass =
        env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&handoff_name)])?.l()?.into();
    env.call_static_method(&handoff, "requestPocket", "(Z)V", &[JValue::Bool(pocket as jni::sys::jboolean)])?;
    Ok(())
}

/// Why a hand-off call failed, for the log line: the pending Java exception's class and message —
/// which is what names a class the loader can't find, since JNI reports that as a bare
/// `JavaException` — or the Rust-side error when nothing was pending. A throwable can only be read
/// once the pending exception is cleared, so this clears it first: leaving one pending would make the
/// JVM rethrow it into unrelated code later.
fn failure_note(env: &mut JNIEnv, why: &jni::errors::Error) -> String {
    let Ok(thrown) = env.exception_occurred() else { return format!("{why}") };
    env.exception_clear().ok();
    let class_name = env.get_object_class(&thrown).ok().and_then(|c| java_string(env, c, "getName"));
    let message = java_string(env, &thrown, "getMessage");
    match (class_name, message) {
        (Some(class), Some(message)) => format!("{class}: {message}"),
        (Some(class), None) => class,
        (None, Some(message)) => message,
        (None, None) => format!("{why}"),
    }
}

/// Read a Java method that returns a String, `None` when the call fails or returns null.
fn java_string<'other_local, O>(env: &mut JNIEnv, obj: O, method: &str) -> Option<String>
where
    O: AsRef<JObject<'other_local>>,
{
    let value = env.call_method(obj, method, "()Ljava/lang/String;", &[]).ok()?;
    let s: JString = value.l().ok()?.into();
    env.get_string(&s).ok().map(String::from)
}
