# Building the Compose phone app

The Rust half is `crates/android-bridge` in this workspace; this folder is the Kotlin/Gradle app
that loads it. One Gradle command builds both: it compiles the bridge with `cargo-ndk`, generates
the Kotlin bindings from the built library (`bindgen/`, the UniFFI generator this workspace also
builds), and packs the result.

```sh
cd apps/wordcraft-android-compose/android
./gradlew assembleRelease                 # or installRelease, or open this folder in Android Studio
./gradlew -Pwordcraft.abis=arm64-v8a assembleDebug   # one ABI is much faster
```

## What it needs

| Tool | Version | Why |
|---|---|---|
| rustup + the Android target | `aarch64-linux-android` (`rust-toolchain.toml`) | compiles the bridge |
| `cargo-ndk` | 4.1.2 | `cargo ndk build -p wordcraft-android-bridge` |
| Android SDK | platform 37 (API 37) | `compileSdk`/`targetSdk` |
| Android NDK | **29.0.14206865** (`ndkVersion` in `app/build.gradle.kts`) | the linker cargo-ndk uses |
| JDK | **17 to 21** | AGP's `compileDebugJavaWithJavac` needs a compiler-capable JDK; 25 is refused |

## What was verified here, and what was not

Verified on the development machine (2026-11-05):

- `cargo build -p wordcraft-android-bridge` — the bridge builds natively, and its 52 tests pass.
- **The bridge cross-compiles for Android**: `cargo ndk -t arm64-v8a --platform 29 build --release
  -p wordcraft-android-bridge --lib` produced a real
  `ELF 64-bit LSB shared object, ARM aarch64, for Android 29, built by NDK r30`.
- **The bindings generate from that library**: `cargo run -p wordcraft-android-bindgen -- generate
  <the .so> --language kotlin --out-dir out` wrote
  `io/github/artcraftandroid/wordcraft/core/wordcraft_android_bridge.kt` (5,678 lines), in the
  package the Kotlin sources import.
- `cargo xtask assets` passes with every icon attributed, and `cargo xtask layers` accepts both new
  crates.

**Not yet verified: a full APK build.** Gradle configures the project, but `:app:assembleDebug`
cannot finish on the development machine for two environment reasons, neither of them a defect in
this tree:

1. The build pins `ndkVersion = "29.0.14206865"`; the machine has NDK `30.0.16248370`, so Gradle
   reports "NDK is not installed" before the cargo task can run.
2. The only JDK present is 25, and AGP refuses it for `compileDebugJavaWithJavac` ("Toolchain
   installation '/usr/lib/jvm/java-25-openjdk' does not provide the required capabilities:
   [JAVA_COMPILER]"). The build targets Java 17 bytecode.

Install NDK 29.0.14206865 and a JDK 17–21 to build the APK.
