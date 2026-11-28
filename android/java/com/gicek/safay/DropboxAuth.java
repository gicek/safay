package com.gicek.safay;

import android.app.Activity;
import android.app.Application;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.content.pm.Signature;
import android.content.pm.SigningInfo;
import android.os.Build;
import java.security.MessageDigest;
import java.util.List;

/**
 * Signing in to Dropbox through its official app (src/platform/dropbox_app_android.rs): the request goes only to a
 * {@code com.dropbox.android} that handles the authorization action and is signed with Dropbox's production
 * certificate, so another app posing as it never gets it. The app answers by starting {@link DropboxAuthActivity},
 * which hands the answer to Rust ({@link #answer}); Rust takes it only for the sign-in it is waiting for (its state).
 *
 * <p>The protocol is the official SDK's ({@code dropbox-sdk-java}'s {@code AuthActivity}, MIT), which is not used: it
 * runs the whole sign-in, token exchange included, where Safay's core (cloudset) builds and checks every request.
 */
public final class DropboxAuth {
    static final String PACKAGE = "com.dropbox.android";
    static final String ACTION = "com.dropbox.android.AUTHENTICATE_V2";
    /**
     * SHA-256 of Dropbox's production signing certificate (the official SDK's {@code DROPBOX_APP_SIGNATURES}); the
     * "Android Debug" certificate the SDK also lists is not trusted.
     */
    private static final String CERTIFICATE_SHA256 = "3467b909be2a577043bb77406388320df882276c937d09689c4d894193984c01";
    /** The SDK version reported to the app, which decides from it what the caller understands. */
    private static final String SDK_VERSION = "7.0.0";

    static final String EXTRA_AUTH_STATE = "AUTH_STATE";
    static final String EXTRA_ACCESS_TOKEN = "ACCESS_TOKEN";
    static final String EXTRA_ACCESS_SECRET = "ACCESS_SECRET";

    private DropboxAuth() {}

    /** The app's answer: the token type, the authorization code and the state. */
    static native void answer(String tokenType, String code, String state);

    /**
     * Sends the request to the official Dropbox app, from the activity in front. False when it is not installed, not
     * Dropbox's, or cannot be started: the browser signs in instead. Any thread.
     */
    public static boolean start(String appKey, String authState, String queryParams) {
        Application app = Bridge.application();
        if (app == null || !official(app)) {
            return false;
        }
        Intent intent = new Intent(ACTION).setPackage(PACKAGE);
        intent.putExtra("CONSUMER_KEY", appKey);
        intent.putExtra("CONSUMER_SIG", "");
        intent.putExtra("CALLING_PACKAGE", app.getPackageName());
        intent.putExtra("CALLING_CLASS", DropboxAuthActivity.class.getName());
        intent.putExtra(EXTRA_AUTH_STATE, authState);
        intent.putExtra("AUTH_QUERY_PARAMS", queryParams);
        intent.putExtra("ALREADY_AUTHED_UIDS", new String[0]);
        intent.putExtra("DROPBOX_SDK_JAVA_VERSION", SDK_VERSION);
        intent.putExtra("TARGET_SDK_VERSION", app.getApplicationInfo().targetSdkVersion);
        // As Bridge.openCustomTab, from the calling thread: the activity in front, else the application.
        Activity front = Bridge.front();
        Context context = front != null ? front : app;
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

    /** Hands an answer {@link DropboxAuthActivity} was started with to Rust; false when it is none. */
    static boolean deliver(Intent intent) {
        if (intent == null || !intent.hasExtra(EXTRA_AUTH_STATE)) {
            return false;
        }
        answer(
            intent.getStringExtra(EXTRA_ACCESS_TOKEN),
            intent.getStringExtra(EXTRA_ACCESS_SECRET),
            intent.getStringExtra(EXTRA_AUTH_STATE));
        return true;
    }

    /**
     * The official app is installed, one of its activities handles the action, and its signing certificates (the
     * rotation history included) have Dropbox's production one. Android 9 and later, which tell a package's signers.
     */
    private static boolean official(Context context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
            return false;
        }
        PackageManager manager = context.getPackageManager();
        List<ResolveInfo> handlers = manager.queryIntentActivities(new Intent(ACTION).setPackage(PACKAGE), 0);
        if (handlers == null || handlers.size() != 1) {
            return false;
        }
        SigningInfo signing;
        try {
            signing = manager.getPackageInfo(PACKAGE, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo;
        } catch (PackageManager.NameNotFoundException e) {
            return false;
        }
        if (signing == null) {
            return false;
        }
        Signature[] certificates = signing.hasMultipleSigners() ? signing.getApkContentsSigners() : signing.getSigningCertificateHistory();
        if (certificates == null) {
            return false;
        }
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            for (Signature certificate : certificates) {
                if (CERTIFICATE_SHA256.equals(hex(digest.digest(certificate.toByteArray())))) {
                    return true;
                }
            }
        } catch (java.security.NoSuchAlgorithmException e) {
            return false;
        }
        return false;
    }

    private static String hex(byte[] bytes) {
        StringBuilder text = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            text.append(String.format("%02x", b & 0xff));
        }
        return text.toString();
    }
}
