package org.flos.phira

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.util.DisplayMetrics
import android.util.Log
import android.view.View
import android.view.Window
import android.view.WindowManager
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import moe.mivik.inputbox.InputBox
import quad_native.QuadNative

/**
 * Thin host for the Rust game.
 *
 * Everything the engine cannot do by itself lives here: the surface host, the file pickers,
 * the clipboard, the browser hand-off and the fullscreen flag. There is deliberately no game
 * logic — the JNI surface in [QuadNative] is the whole contract.
 */
class MainActivity : AppCompatActivity() {
    private val importChartLauncher: ActivityResultLauncher<Intent> =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            if (result.resultCode == RESULT_OK) {
                result.data?.data?.let { sendFileToNative(this, it) }
            }
        }

    private val exportFileLauncher: ActivityResultLauncher<String> =
        registerForActivityResult(ActivityResultContracts.CreateDocument("*/*")) { uri ->
            if (uri == null) return@registerForActivityResult
            val pfd = contentResolver.openFileDescriptor(uri, "w") ?: return@registerForActivityResult
            try {
                QuadNative.processExportFd(uri, pfd.detachFd())
            } finally {
                pfd.close()
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        supportRequestWindowFeature(Window.FEATURE_NO_TITLE)
        super.onCreate(savedInstanceState)

        val surface = QuadSurface(this)
        setContentView(surface)

        WindowCompat.getInsetsController(window, surface).apply {
            hide(WindowInsetsCompat.Type.statusBars())
            hide(WindowInsetsCompat.Type.navigationBars())
        }

        QuadNative.setDataPath(filesDir.absolutePath)
        QuadNative.setTempDir(cacheDir.absolutePath)
        val metrics: DisplayMetrics = resources.displayMetrics
        QuadNative.setDpi(minOf(metrics.xdpi, metrics.ydpi).toInt())
        // The engine's text fields live behind `inputbox`, which needs a live Activity.
        InputBox.attach(application)
        InputBox.setActivity(this)
        QuadNative.initializeContext(this)
        // Enters the engine loop; it only returns when the process is going away.
        QuadNative.activityOnCreate(this)
    }

    override fun onResume() {
        super.onResume()
        QuadNative.activityOnResume()
        QuadNative.prprActivityOnResume()
    }

    override fun onPause() {
        super.onPause()
        QuadNative.activityOnPause()
        QuadNative.prprActivityOnPause()
    }

    override fun onDestroy() {
        super.onDestroy()
        QuadNative.releaseContext()
        QuadNative.activityOnDestroy()
        QuadNative.prprActivityOnDestroy()
    }

    // ---- called from Rust ------------------------------------------------------------

    /** Opens the system document picker; the result goes through [sendFileToNative]. */
    fun chooseFile() {
        runOnUiThread {
            importChartLauncher.launch(
                Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "*/*"
                },
            )
        }
    }

    /** Shows a single-line text prompt and hands the result back to the engine. */
    fun inputText(initial: String) {
        runOnUiThread {
            showInput(this, "输入", "文字", initial) {
                QuadNative.setInputText(it)
                null
            }
        }
    }

    /** Asks the picker for a destination, then lets Rust stream the blob into that fd. */
    fun showExportDialog(suggestedName: String) {
        runOnUiThread { exportFileLauncher.launch(suggestedName) }
    }

    fun openUrl(url: String) {
        runOnUiThread { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
    }

    fun copy(text: String) {
        val manager = getSystemService(CLIPBOARD_SERVICE) as ClipboardManager
        manager.setPrimaryClip(ClipData.newPlainText("text", text))
    }

    /** Deletes a file the engine failed to write, so the picker's entry does not linger. */
    fun deleteUri(uri: Uri) {
        runOnUiThread {
            try {
                DocumentsContract.deleteDocument(contentResolver, uri)
                Log.i(TAG, "Native write failed, temporary file deleted.")
            } catch (e: Exception) {
                Log.e(TAG, "Failed to delete corrupted file", e)
            }
        }
    }

    fun setFullScreen(fullscreen: Boolean) {
        runOnUiThread {
            val decor: View = window.decorView
            if (!fullscreen) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) window.setDecorFitsSystemWindows(true)
                return@runOnUiThread
            }
            window.setFlags(
                android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN,
                android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN,
            )
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                window.attributes.layoutInDisplayCutoutMode =
                    android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                window.setDecorFitsSystemWindows(false)
            } else {
                @Suppress("DEPRECATION")
                decor.systemUiVisibility = (View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                    or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                    or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                    or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    or View.SYSTEM_UI_FLAG_FULLSCREEN
                    or View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY)
            }
        }
    }

    // ---- regional login --------------------------------------------------------------
    //
    // The upstream global build ships these three as empty stubs: the 好游快爆 (HYKB) SDK is
    // only linked in the Chinese channel build. They are kept so the Rust callbacks that
    // reference them still resolve.

    fun hykbLogin() = runOnUiThread { /* no SDK in this build */ }

    fun hykbLogout() = runOnUiThread { /* no SDK in this build */ }

    fun hykbSwitchAccount() = runOnUiThread { /* no SDK in this build */ }

    private companion object {
        const val TAG = "SAPP"
    }
}
