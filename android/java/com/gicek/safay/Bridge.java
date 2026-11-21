package com.gicek.safay;

import android.app.Activity;
import android.app.Application;
import android.content.BroadcastReceiver;
import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.PersistableBundle;
import android.view.WindowManager;

/**
 * What Safay's Rust code asks of Android where an API takes a class of the app: the activity in front, the clipboard, the
 * screen going off. Rust calls the static methods; the natives are in libnsh.so (src/platform/android.rs).
 */
public final class Bridge {
    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    private static volatile Application application;
    private static boolean started;
    private static Activity front;
    private static boolean secure;
    private static int copies;

    private Bridge() {}

    /** The screen went off: the vault locks at once. */
    private static native void screenOff();

    /** Called once, from any thread, with the application. */
    public static void start(final Application app) {
        // Known at once: what asks for it next (whether a biometric is enrolled) may run before the main thread does.
        application = app;
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                if (started) {
                    return;
                }
                started = true;
                // The activity that is up already: the callbacks below tell only of what happens from now on.
                front = running();
                if (front != null) {
                    apply(front);
                }
                app.registerActivityLifecycleCallbacks(new Application.ActivityLifecycleCallbacks() {
                    @Override
                    public void onActivityCreated(Activity activity, Bundle state) {
                        front = activity;
                        apply(activity);
                    }

                    @Override
                    public void onActivityStarted(Activity activity) {}

                    @Override
                    public void onActivityResumed(Activity activity) {
                        front = activity;
                        apply(activity);
                    }

                    @Override
                    public void onActivityPaused(Activity activity) {}

                    @Override
                    public void onActivityStopped(Activity activity) {}

                    @Override
                    public void onActivitySaveInstanceState(Activity activity, Bundle state) {}

                    @Override
                    public void onActivityDestroyed(Activity activity) {
                        if (front == activity) {
                            front = null;
                        }
                    }
                });
                app.registerReceiver(new BroadcastReceiver() {
                    @Override
                    public void onReceive(Context context, Intent intent) {
                        screenOff();
                    }
                }, new IntentFilter(Intent.ACTION_SCREEN_OFF));
            }
        });
    }

    /** The activity the app is showing now, from the activity thread's own list; null when it cannot be read. */
    private static Activity running() {
        try {
            Class<?> type = Class.forName("android.app.ActivityThread");
            Object thread = type.getMethod("currentActivityThread").invoke(null);
            java.lang.reflect.Field records = type.getDeclaredField("mActivities");
            records.setAccessible(true);
            for (Object record : ((java.util.Map<?, ?>) records.get(thread)).values()) {
                java.lang.reflect.Field activity = record.getClass().getDeclaredField("activity");
                activity.setAccessible(true);
                Activity found = (Activity) activity.get(record);
                if (found != null && !found.isFinishing()) {
                    return found;
                }
            }
        } catch (Exception | LinkageError ignored) {
            // Not readable on this system: the next activity to resume is taken instead.
        }
        return null;
    }

    /** The activity in front, for a prompt; null in the background. Main thread. */
    static Activity front() {
        return front;
    }

    static Application application() {
        return application;
    }

    /**
     * Opens `url` in a Custom Tab: a view intent carrying the Custom Tabs session extra (null: no session), which a
     * browser that supports them shows as a tab over the app, in the app's task. Whether an app took it.
     */
    public static boolean openCustomTab(String url) {
        android.content.Context context = front != null ? front : application;
        if (context == null) {
            return false;
        }
        Intent intent = new Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url));
        Bundle extras = new Bundle();
        extras.putBinder("android.support.customtabs.extra.SESSION", null);
        intent.putExtras(extras);
        if (!(context instanceof Activity)) {
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        }
        try {
            context.startActivity(intent);
            return true;
        } catch (RuntimeException e) {
            return false;
        }
    }

    /** Keeps the app's windows out of screenshots, recordings and the recent apps' pictures while `on`. */
    public static void setSecure(final boolean on) {
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                secure = on;
                if (front != null) {
                    apply(front);
                }
            }
        });
    }

    private static void apply(Activity activity) {
        if (secure) {
            activity.getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        } else {
            activity.getWindow().clearFlags(WindowManager.LayoutParams.FLAG_SECURE);
        }
    }

    /**
     * Puts `text` on the clipboard marked sensitive (the system's preview hides it), and takes it off again after
     * `ttlMillis` unless something else was copied meanwhile.
     */
    public static void copySensitive(final String text, final long ttlMillis) {
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                if (application == null) {
                    return;
                }
                final ClipboardManager clipboard = (ClipboardManager) application.getSystemService(Context.CLIPBOARD_SERVICE);
                ClipData clip = ClipData.newPlainText("", text);
                PersistableBundle extras = new PersistableBundle();
                // ClipDescription.EXTRA_IS_SENSITIVE (API 33); the same key is read by earlier keyboards.
                extras.putBoolean("android.content.extra.IS_SENSITIVE", true);
                clip.getDescription().setExtras(extras);
                clipboard.setPrimaryClip(clip);
                final int mine = ++copies;
                MAIN.postDelayed(new Runnable() {
                    @Override
                    public void run() {
                        // Only the last copy of Safay's is cleared, and only while the clipboard still holds it: reading
                        // the clip is allowed in the foreground only, so in the background it is cleared unread.
                        if (mine != copies) {
                            return;
                        }
                        try {
                            ClipData now = clipboard.getPrimaryClip();
                            if (now != null && now.getItemCount() > 0 && !text.contentEquals(String.valueOf(now.getItemAt(0).getText()))) {
                                return;
                            }
                        } catch (RuntimeException ignored) {
                            // Not readable from the background: cleared below.
                        }
                        if (Build.VERSION.SDK_INT >= 28) {
                            clipboard.clearPrimaryClip();
                        } else {
                            clipboard.setPrimaryClip(ClipData.newPlainText("", ""));
                        }
                    }
                }, ttlMillis);
            }
        });
    }
}
