package org.flos.phira

import android.content.Context
import android.content.DialogInterface
import android.net.Uri
import android.util.TypedValue
import android.widget.FrameLayout
import androidx.appcompat.app.AlertDialog
import com.google.android.material.textfield.TextInputEditText
import com.google.android.material.textfield.TextInputLayout
import quad_native.QuadNative
import java.io.File
import java.io.IOException

/** `dp` in pixels. */
fun Context.dp(value: Float): Int =
    TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, resources.displayMetrics).toInt()

/**
 * Copies a picked document into the cache directory and tells the engine where it is.
 *
 * The picker hands out a `content://` URI that the game must not hold on to, so the bytes are
 * materialised into a real file first. The copy is chunked rather than `readBytes()` because a
 * chart pack can be hundreds of megabytes.
 */
@Throws(IOException::class)
fun sendFileToNative(context: Context, uri: Uri) {
    val file = File.createTempFile("chart", ".zip")
    val input = context.contentResolver.openInputStream(uri)
        ?: throw IllegalStateException("无法读取文件")
    input.use { source ->
        file.outputStream().use { sink ->
            val buffer = ByteArray(8192)
            while (true) {
                val n = source.read(buffer)
                if (n < 0) break
                sink.write(buffer, 0, n)
            }
        }
    }
    QuadNative.setChosenFile(file.absolutePath)
}

/**
 * A one-field prompt.
 *
 * [callback] returns `null` to accept the value (the dialog closes) or an error string that is
 * shown under the field, so validation can loop without dismissing the dialog.
 */
fun showInput(
    context: Context,
    title: String,
    hint: String,
    text: String,
    callback: (String) -> String?,
) {
    val layout = TextInputLayout(context).apply { this.hint = hint }
    val edit = TextInputEditText(context).apply { setText(text) }
    layout.addView(edit)

    val padding = context.dp(5f)
    val container = FrameLayout(context).apply {
        setPadding(padding, padding, padding, padding)
        addView(layout)
    }

    val dialog = AlertDialog.Builder(context)
        .setTitle(title)
        .setView(container)
        .setNegativeButton("取消", null)
        .setPositiveButton("确定", null)
        .setCancelable(true)
        .create()

    dialog.setOnShowListener {
        dialog.getButton(DialogInterface.BUTTON_POSITIVE).setOnClickListener {
            val error = callback(edit.text?.toString().orEmpty())
            if (error == null) dialog.dismiss() else layout.error = error
        }
    }
    dialog.show()
}
