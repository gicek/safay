package com.gicek.safay;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

/**
 * Where the official Dropbox app answers a sign-in ({@link DropboxAuth}): transparent, out of the recent apps and the
 * back stack. It hands the answer on, brings Safay's own activity back to the front and finishes at once.
 */
public final class DropboxAuthActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        DropboxAuth.deliver(getIntent());
        Intent back = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (back != null) {
            back.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP);
            startActivity(back);
        }
        finish();
    }
}
