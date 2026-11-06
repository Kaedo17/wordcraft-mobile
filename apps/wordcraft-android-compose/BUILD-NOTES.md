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
- **Both front ends' libraries cross-compile**, which is what the one-APK hand-off needs: the same
  command with `-p wordcraft-android` produced `libwordcraft_android.so` (80 MB, the egui front end
  over eframe/winit/wgpu) with `android_main` exported — the symbol the `NativeActivity` declared in
  the manifest loads. A full link needs about 5 GB of free RAM; with less, cargo reports it is
  waiting on the build directory and the link can be killed, so build it when the machine has room
  (`CARGO_BUILD_JOBS=1` helps).
- **The bindings generate from that library**: `cargo run -p wordcraft-android-bindgen -- generate
  <the .so> --language kotlin --out-dir out` wrote
  `io/github/artcraftandroid/wordcraft/core/wordcraft_android_bridge.kt` (5,678 lines), in the
  package the Kotlin sources import.
- `cargo xtask assets` passes with every icon attributed, and `cargo xtask layers` accepts both new
  crates.

**The APK is built by CI, not by this machine.** Gradle configures the project, and the pinned
NDK (`29.0.14206865`, installed locally by `sdkmanager`) lets the cargo task reach the linker,
but `:app:assembleDebug` still cannot finish here for one environment reason, not a defect in
this tree: the only Java present is the OpenJDK **25 JRE** (`/usr/lib/jvm/java-25-openjdk`, no
`javac`), and AGP needs a JDK 17–21 for `compileDebugJavaWithJavac` ("does not provide the
required capabilities: [JAVA_COMPILER]"). The build targets Java 17 bytecode.

So the APK ships from the GitHub Actions workflow (`.github/workflows/android.yml`), which
installs JDK 17, NDK 29.0.14206865 and cargo-ndk 4.1.2, builds both libraries with
`cargo ndk`, assembles with `-Pwordcraft.abis=arm64-v8a`, checks both `.so` files and both
activities in the packaged manifest, and uploads the `wordcraft-apk` artifact. That artifact has
been installed and run on a phone (POCO X6 Pro, Android 17/API 37) — see
[`docs/pocket-mode.md`](../docs/pocket-mode.md) for what the device run verified. Installing a
CI debug APK over a locally-signed one needs `adb uninstall` first (the debug keystores differ),
and that uninstall wipes both front ends' app-private storage.
