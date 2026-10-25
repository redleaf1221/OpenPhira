package moe.mivik.inputbox;

import android.app.Activity;
import android.app.Application;
import android.content.DialogInterface;
import android.os.Bundle;
import android.text.InputType;
import android.util.TypedValue;
import android.view.Gravity;
import android.widget.FrameLayout;

import androidx.appcompat.app.AlertDialog;

import com.google.android.material.textfield.TextInputEditText;
import com.google.android.material.textfield.TextInputLayout;

/**
 * The Java half of the {@code inputbox} crate's Android backend.
 *
 * <p>The Rust side calls {@link #showInput} with a boxed callback pointer, and expects exactly
 * one {@link #inputCallback} call back — either the entered text, or {@code null} when the user
 * cancelled. Calling it twice would free the same box twice, so the dialog funnels both the
 * "OK" and "dismissed" paths through one guard.
 *
 * <p>{@code inputCallback} is native and its symbol is resolved by name
 * ({@code Java_moe_mivik_inputbox_InputBox_inputCallback}), which is why this class is
 * hand-ported in Java rather than generated from Kotlin.
 */
public final class InputBox {
    private static volatile Activity current;

    private InputBox() {
    }

    /** Mirrors {@code Java_moe_mivik_inputbox_InputBox_inputCallback}. */
    public static native void inputCallback(long id, String text);

    /**
     * Shows the prompt and returns {@code null} if the dialog was scheduled, or a message
     * describing why it could not be (which the Rust side turns into an error).
     */
    public static String showInput(
            long id,
            String title,
            String prompt,
            String defaultText,
            String okLabel,
            String cancelLabel,
            String mode,
            boolean autoWrap,
            boolean scrollToEnd) {
        Activity activity = current;
        if (activity == null) {
            return "no active activity";
        }

        activity.runOnUiThread(() -> buildAndShow(
                activity, id, title, prompt, defaultText, okLabel, cancelLabel, mode, autoWrap));
        return null;
    }

    private static void buildAndShow(
            Activity activity,
            long id,
            String title,
            String prompt,
            String defaultText,
            String okLabel,
            String cancelLabel,
            String mode,
            boolean autoWrap) {
        TextInputEditText edit = new TextInputEditText(activity);
        edit.setText(defaultText);

        switch (mode == null ? "text" : mode) {
            case "password":
                edit.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
                break;
            case "multiline":
                edit.setInputType(autoWrap
                        ? InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                        : InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
                if (!autoWrap) {
                    edit.setHorizontallyScrolling(true);
                    edit.setMaxLines(10);
                }
                edit.setMinLines(3);
                edit.setGravity(Gravity.TOP | Gravity.START);
                break;
            default:
                edit.setInputType(InputType.TYPE_CLASS_TEXT);
                break;
        }

        TextInputLayout layout = new TextInputLayout(activity);
        layout.setHint(prompt);
        layout.addView(edit);

        int padding = (int) TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP, 5f, activity.getResources().getDisplayMetrics());
        FrameLayout container = new FrameLayout(activity);
        container.setPadding(padding, padding, padding, padding);
        container.addView(layout);

        // The dialog can finish through either button or through a dismiss; only the first
        // one may release the Rust callback.
        final boolean[] done = {false};

        AlertDialog dialog = new AlertDialog.Builder(activity)
                .setTitle(title)
                .setView(container)
                .setPositiveButton(okLabel, (d, which) -> {
                    if (done[0]) return;
                    done[0] = true;
                    inputCallback(id, String.valueOf(edit.getText()));
                })
                .setNegativeButton(cancelLabel, null)
                .setOnDismissListener(d -> {
                    if (done[0]) return;
                    done[0] = true;
                    inputCallback(id, null);
                })
                .create();
        dialog.show();
    }

    /** Keeps the most recently resumed Activity, which is where the dialog is attached. */
    public static void setActivity(Activity activity) {
        current = activity;
    }

    /** Registers the lifetime callbacks that keep {@link #current} up to date. */
    public static void attach(Application application) {
        application.registerActivityLifecycleCallbacks(new Application.ActivityLifecycleCallbacks() {
            @Override
            public void onActivityCreated(Activity activity, Bundle savedInstanceState) {
            }

            @Override
            public void onActivityStarted(Activity activity) {
            }

            @Override
            public void onActivityResumed(Activity activity) {
                current = activity;
            }

            @Override
            public void onActivityPaused(Activity activity) {
                if (current == activity) current = null;
            }

            @Override
            public void onActivityStopped(Activity activity) {
            }

            @Override
            public void onActivitySaveInstanceState(Activity activity, Bundle outState) {
            }

            @Override
            public void onActivityDestroyed(Activity activity) {
                if (current == activity) current = null;
            }
        });
    }
}
