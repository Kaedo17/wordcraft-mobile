package io.github.artcraftandroid.wordcraft

import android.app.NativeActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/**
 * The desktop front end's activity: egui's `NativeActivity`, plus one thing Android's own class
 * does not do — hide the phone's system bars. egui's ribbon tabs sit at the very top of the window,
 * and the status bar draws over them and takes the touches, so a finger could never reach View ›
 * Views › Pocket Mode, which is the only way back to the phone interface once Desktop Mode is
 * remembered. The bars come back on a swipe from an edge, so Back is still there.
 *
 * The window is untouched otherwise: `android.app.lib_name` in the manifest still points at
 * `libwordcraft_android.so`, and winit drives the event loop as before. The compat controller covers
 * every API this app supports, so nothing here depends on the device's Android version.
 */
class DesktopActivity : NativeActivity() {
    override fun onResume() {
        super.onResume()
        hideSystemBars()
    }

    /** Full-screen immersive: bars hidden, transient on a swipe from an edge. */
    private fun hideSystemBars() {
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        runCatching { controller.hide(WindowInsetsCompat.Type.systemBars()) }
    }
}
