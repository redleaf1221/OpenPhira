package org.flos.phira

import android.content.Intent
import android.os.Bundle
import android.util.Log
import androidx.appcompat.app.AppCompatActivity
import quad_native.QuadNative

/**
 * Same hand-off as [ImportActivity] but for resource packs, which the engine installs instead
 * of a single chart. The two are separate activities because the intent filters would
 * otherwise be indistinguishable.
 */
class ImportRespackActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val data = intent.data
        if (data == null) {
            finish()
            return
        }
        try {
            sendFileToNative(this, data)
            QuadNative.markImportRespack()
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
