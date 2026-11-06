package io.github.artcraftandroid.wordcraft

import android.app.NativeActivity
import android.os.Build
import android.window.WindowInsets
import android.window.WindowInsetsController

/**
 * The desktop front end's activity: egui's `NativeActivity`, plus one thing Android's own class
 * does not do — hide the phone's system bars. egui's ribbon tabs sit at the very top of the window,
 * and the status bar draws over them and takes the touches, so a finger could never reach View ›
 * Views › Pocket Mode, which is the only way back to the phone interface once Desktop Mode is
 * remembered. The bars come back on a swipe from an edge, so Back is still there.
 *
 * The window itself is untouched otherwise: `android.app.lib_name` in the manifest still points at
 * `libwordcraft_android.so`, and winit drives the event loop as before.
 */
class DesktopActivity : NativeActivity() {
    override fun onResume() {
        super.onResume()
        hideSystemBars()
    }

    /** Full-screen immersive: bars hidden, transient on an edge swipe. Below API 30 there is no
     * insets controller, so the bars stay as Android draws them — nothing else changes. */
    private fun hideSystemBars() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
        val controller = window.insetsController ?: return
        controller.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        runCatching { controller.hide(WindowInsets.Type.systemBars()) }
    }
}
