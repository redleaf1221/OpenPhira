package org.flos.phira

import android.content.Context
import android.view.InputDevice
import android.view.MotionEvent
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import quad_native.QuadNative

/**
 * The one view of the game: a [SurfaceView] that owns the native window and converts
 * [MotionEvent]s into the flat `(id, action, x, y, time)` form the renderer expects.
 */
class QuadSurface(context: Context) : SurfaceView(context), View.OnTouchListener, SurfaceHolder.Callback {
    init {
        holder.addCallback(this)
        isFocusable = true
        isFocusableInTouchMode = true
        requestFocus()
        setOnTouchListener(this)
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        QuadNative.surfaceOnSurfaceCreated(holder.surface)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        QuadNative.surfaceOnSurfaceChanged(holder.surface, width, height)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        QuadNative.surfaceOnSurfaceDestroyed(holder.surface)
    }

    override fun onTouch(view: View, event: MotionEvent): Boolean {
        val time = event.eventTime
        val device: InputDevice? = InputDevice.getDevice(event.deviceId)

        // Sampled once per event, before the action dispatch: the probe is stateful, so the
        // order of these calls is part of what the server verifies.
        QuadNative.preprocessInput(
            event.deviceId,
            device?.isExternal ?: false,
            device?.isVirtual ?: false,
        )

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                val i = if (event.actionMasked == MotionEvent.ACTION_DOWN) 0 else event.actionIndex
                QuadNative.surfaceOnTouch(event.getPointerId(i), ACTION_DOWN, event.getX(i), event.getY(i), time)
            }

            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                val i = if (event.actionMasked == MotionEvent.ACTION_UP) 0 else event.actionIndex
                QuadNative.surfaceOnTouch(event.getPointerId(i), ACTION_UP, event.getX(i), event.getY(i), time)
            }

            MotionEvent.ACTION_MOVE -> {
                for (i in 0 until event.pointerCount) {
                    QuadNative.surfaceOnTouch(event.getPointerId(i), ACTION_MOVE, event.getX(i), event.getY(i), time)
                }
            }

            MotionEvent.ACTION_CANCEL -> {
                for (i in 0 until event.pointerCount) {
                    QuadNative.surfaceOnTouch(event.getPointerId(i), ACTION_CANCEL, event.getX(i), event.getY(i), time)
                }
            }

            else -> return false
        }
        return true
    }

    private companion object {
        // Values consumed by miniquad's `surfaceOnTouch`: they are *not* the MotionEvent
        // constants (1/0/2/3 here vs 1/2/0/3 there), so they are spelled out.
        const val ACTION_MOVE = 0
        const val ACTION_UP = 1
        const val ACTION_DOWN = 2
        const val ACTION_CANCEL = 3
    }
}
