package io.github.artcraftandroid.wordcraft

import android.app.Application
import android.content.Intent
import android.system.Os
import io.github.artcraftandroid.wordcraft.core.MobileSession
import io.github.artcraftandroid.wordcraft.core.initLogging
import io.github.artcraftandroid.wordcraft.engine.Engine
import java.io.File
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.launch

class WordcraftApplication : Application() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** The phone front end's activity while it exists; the desktop front end's report finishes it. */
    var phoneActivity: MainActivity? = null

    /** The settings the phone app and the hand-off both read: the same file, the same keys. */
    private val settingsPrefs by lazy { getSharedPreferences("settings", MODE_PRIVATE) }

    /** Hand-offs switch activities, and Android can drop one started from a background thread. */
    private val handoffScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    override fun onCreate() {
        super.onCreate()
        // Pocket Mode's choice, posted from egui's native thread (`FrontEndHandoff.requestPocket`):
        // which front end shows now, and — remembered in `desktopInterface` — which one the app
        // opens on next launch. Drained here, not in MainActivity, because that activity may be
        // finished while egui is on screen and the request still has to switch back.
        handoffScope.launch {
            for (pocket in FrontEndHandoff.requests) {
                settingsPrefs.edit().putBoolean("desktopInterface", !pocket).apply()
                val switch = if (pocket) {
                    Intent(this@WordcraftApplication, MainActivity::class.java)
                } else {
                    Intent().setClassName(packageName, "android.app.NativeActivity")
                }
                switch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT)
                runCatching { startActivity(switch) }
            }
        }
        // egui reports its window is up only once its event loop and window exist. Then the phone
        // activity that was started underneath it is retired: kept in the task, the back gesture
        // uncovered Pocket Mode "like another page" instead of leaving WordCraft. A desktop front
        // end that cannot start never reports, so the phone interface stays on screen.
        handoffScope.launch {
            for (up in FrontEndHandoff.interfaceUp) {
                if (settingsPrefs.getBoolean("desktopInterface", false)) runCatching { phoneActivity?.finish() }
            }
        }
    }

    /** The engine, loaded once per process on a background thread and shared by the screens. */
    val engine: Deferred<Result<Engine>> by lazy {
        scope.async {
            runCatching {
                // Rust's home and temporary folder on Android are not the app's own: point them at
                // it before the library first reads them (it can't set them itself). The engine
                // also looks for fonts in HOME/.fonts, where the app puts the ones it ships.
                installDocumentFonts(File(filesDir, ".fonts"))
                Os.setenv("HOME", filesDir.absolutePath, true)
                Os.setenv("TMPDIR", cacheDir.absolutePath, true)
                initLogging()
                val session = MobileSession()
                session.start(Locale.getDefault().toLanguageTag())
                Engine(session)
            }
        }
    }

    /**
     * The fonts the app ships for documents (assets/docfonts, open fonts with the widths of the
     * ones Word documents ask for), copied where the engine looks for them; again only when a file
     * changed.
     */
    private fun installDocumentFonts(dir: File) {
        dir.mkdirs()
        val names = runCatching { assets.list("docfonts") }.getOrNull().orEmpty()
        for (name in names) {
            val dest = File(dir, name)
            runCatching {
                val size = assets.openFd("docfonts/$name").use { it.length }
                if (dest.isFile && dest.length() == size) return@runCatching
                assets.open("docfonts/$name").use { input -> dest.outputStream().use { input.copyTo(it) } }
            }.recoverCatching {
                // compressed assets have no file descriptor: copy unconditionally
                if (!dest.isFile) assets.open("docfonts/$name").use { input -> dest.outputStream().use { input.copyTo(it) } }
            }
        }
    }
}
