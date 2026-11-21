package com.gicek.safay;

import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/**
 * Cloud sign-ins at rest: sealed with an AES-256-GCM key that never leaves the hardware keystore, so the files Safay
 * keeps (outside the system's backups) open only on this device, in this app.
 */
public final class Secrets {
    private static final String ALIAS = "safay.cloud.v1";

    private Secrets() {}

    private static SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        SecretKey held = (SecretKey) store.getKey(ALIAS, null);
        if (held != null) {
            return held;
        }
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build());
        return generator.generateKey();
    }

    /** The 12-byte nonce the keystore chose, then the ciphertext with its tag; null when the keystore refuses. */
    public static byte[] seal(byte[] plain) {
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.ENCRYPT_MODE, key());
            byte[] nonce = cipher.getIV();
            byte[] sealed = cipher.doFinal(plain);
            byte[] out = new byte[nonce.length + sealed.length];
            System.arraycopy(nonce, 0, out, 0, nonce.length);
            System.arraycopy(sealed, 0, out, nonce.length, sealed.length);
            return out;
        } catch (Exception e) {
            return null;
        }
    }

    /** What `seal` sealed; null when it does not open (another device's file, a damaged one, a key that is gone). */
    public static byte[] open(byte[] sealed) {
        try {
            if (sealed.length < 12 + 16) {
                return null;
            }
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, sealed, 0, 12));
            return cipher.doFinal(sealed, 12, sealed.length - 12);
        } catch (Exception e) {
            return null;
        }
    }
}
