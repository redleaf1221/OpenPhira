package org.flos.phira

import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.appcompat.app.AppCompatActivity
import quad_native.QuadNative

/**
 * Receives `VIEW` intents for chart packs (`file://` / `content://`, any MIME type), stages the
 * file for the engine and starts the game with the "import" marker set.
 *
 * Android may deliver the same intent to a fresh process, so nothing here may assume that the
 * engine already exists.
 */
class ImportActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val data = intent.data
        if (data == null) {
            finish()
            return
        }
        try {
            sendFileToNative(this, data)
            QuadNative.markImport()
            startActivity(Intent(this, MainActivity::class.java))
            finish()
        } catch (e: Throwable) {
            Log.e(TAG, "Failed to import", e)
        }
    }

    private companion object {
        const val TAG = "SAPP"
    }
}
