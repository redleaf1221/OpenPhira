package quad_native;

import android.content.Context;
import android.net.Uri;
import android.view.MotionEvent;
import android.view.Surface;

/**
 * The whole Java &harr; Rust boundary of Phira.
 *
 * <p>Every symbol here is resolved by name, so the class and method names are part of the
 * ABI: {@code Java_quad_native_QuadNative_preprocessInput} and friends. Changing a package
 * or a method name silently breaks the link at runtime, not at build time.
 *
 * <p>Most of these are implemented by the miniquad fork ({@code prpr-miniquad}), which owns
 * the surface/lifecycle/input side; {@code phira} owns the rest; {@code preprocessInput} is
 * the part that was missing from the open tree and lives in {@code phira::inner::jni}.
 */
public final class QuadNative {
    static {
        System.loadLibrary("phira");
        // Binds the `inputbox` crate's Android backend to moe.mivik.inputbox.InputBox. Must run
        // before the engine is started, because text fields are resolved by class lookup once.
        initializeEnvironment();
    }

    private QuadNative() {
    }

    // ---- bootstrap -------------------------------------------------------------------

    /** Sets up the Android backend of the {@code inputbox} crate. Must run before any UI. */
    public static native void initializeEnvironment();

    /** Hands the JVM and the Activity to {@code ndk-context}. */
    public static native void initializeContext(Context context);

    public static native void releaseContext();

    /** Stores the Activity, then enters {@code quad_main()} — this call does not return. */
    public static native void activityOnCreate(Object activity);

    public static native void activityOnDestroy();

    public static native void activityOnPause();

    public static native void activityOnResume();

    public static native void prprActivityOnResume();

    public static native void prprActivityOnPause();

    public static native void prprActivityOnDestroy();

    // ---- paths and configuration -----------------------------------------------------

    public static native void setDataPath(String path);

    public static native void setTempDir(String path);

    public static native void setDpi(int dpi);

    public static native void setChosenFile(String file);

    public static native void setInputText(String text);

    public static native void markImport();

    public static native void markImportRespack();

    /** Writes the replay/export blob into a file descriptor handed over by the picker. */
    public static native void processExportFd(Uri uri, int fd);

    // ---- input -----------------------------------------------------------------------

    /**
     * Records one touch event for the anti-cheat device probe.
     *
     * <p>The reference client passes the whole {@link MotionEvent} and reads {@code getDeviceId()}
     * inside Rust; we pass the id itself because that number is the only thing the checksum
     * depends on. This is the one deliberate ABI difference from the shipped library.
     *
     * @param deviceId {@code MotionEvent.getDeviceId()}
     * @param external {@code InputDevice.isExternal()} — a real touchscreen reports false
     * @param virtual  {@code InputDevice.isVirtual()} — an injected event reports true
     */
    public static native void preprocessInput(int deviceId, boolean external, boolean virtual);

    public static native void surfaceOnSurfaceCreated(Surface surface);

    public static native void surfaceOnSurfaceChanged(Surface surface, int width, int height);

    public static native void surfaceOnSurfaceDestroyed(Surface surface);

    /**
     * @param touchId pointer id
     * @param action  0 = move, 1 = up, 2 = down, 3 = cancel
     * @param time   {@code MotionEvent.getEventTime()} in milliseconds
     */
    public static native void surfaceOnTouch(int touchId, int action, float x, float y, long time);

    public static native void surfaceOnKeyDown(int keyCode);

    public static native void surfaceOnKeyUp(int keyCode);

    public static native void surfaceOnCharacter(int character);

    // ---- regional builds -------------------------------------------------------------

    /** Chinese anti-addiction result callback; also present in the 0.6.7 ABI. */
    public static native void antiAddictionCallback(int code);
}
