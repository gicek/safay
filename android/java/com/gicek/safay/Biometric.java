package com.gicek.safay;

import android.app.Activity;
import android.content.DialogInterface;
import android.hardware.biometrics.BiometricManager;
import android.hardware.biometrics.BiometricPrompt;
import android.os.Build;
import android.os.CancellationSignal;
import android.os.Handler;
import android.os.Looper;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyPermanentlyInvalidatedException;
import android.security.keystore.KeyProperties;
import java.security.KeyFactory;
import java.security.KeyPairGenerator;
import java.security.KeyStore;
import java.security.PrivateKey;
import java.security.PublicKey;
import java.security.spec.MGF1ParameterSpec;
import java.security.spec.X509EncodedKeySpec;
import java.util.concurrent.Executor;
import javax.crypto.Cipher;
import javax.crypto.spec.OAEPParameterSpec;
import javax.crypto.spec.PSource;

/**
 * Unlocking with a fingerprint or a face: an RSA key in the hardware keystore wraps the vault key. Wrapping uses the
 * public key and asks nothing; unwrapping needs a strong biometric on the spot, each time, and the key is gone for good
 * once the enrolled biometrics change. What the prompt releases is the vault key itself, never a "yes".
 */
public final class Biometric {
    private static final String ALIAS = "safay.biometric.v1";
    private static final String TRANSFORMATION = "RSA/ECB/OAEPWithSHA-256AndMGF1Padding";
    // The results `result` takes.
    private static final int OK = 0, UNAVAILABLE = 1, CANCELLED = 2, FALLBACK = 3, FAILED = 4, LOCKOUT = 5, STALE = 6;
    private static final Handler MAIN = new Handler(Looper.getMainLooper());

    private Biometric() {}

    /** The answer to `open`: a result above, and the vault key when it is OK. */
    private static native void result(long token, int code, byte[] key);

    private static OAEPParameterSpec oaep() {
        // The keystore's OAEP uses SHA-1 for MGF1 whatever the main digest.
        return new OAEPParameterSpec("SHA-256", "MGF1", MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT);
    }

    /** Whether a strong biometric is enrolled and usable. */
    public static boolean available() {
        if (Build.VERSION.SDK_INT < 28 || Bridge.application() == null) {
            return false;
        }
        try {
            BiometricManager manager = Bridge.application().getSystemService(BiometricManager.class);
            if (manager == null) {
                return false;
            }
            if (Build.VERSION.SDK_INT >= 30) {
                return manager.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS;
            }
            return manager.canAuthenticate() == BiometricManager.BIOMETRIC_SUCCESS;
        } catch (RuntimeException e) {
            return false;
        }
    }

    /** Wraps `key` for a later `open`, with a new keystore key; null when the keystore refuses. */
    public static byte[] seal(byte[] key) {
        try {
            KeyStore store = KeyStore.getInstance("AndroidKeyStore");
            store.load(null);
            store.deleteEntry(ALIAS);
            KeyGenParameterSpec.Builder spec = new KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_DECRYPT)
                    .setKeySize(2048)
                    .setDigests(KeyProperties.DIGEST_SHA256, KeyProperties.DIGEST_SHA1)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_RSA_OAEP)
                    .setUserAuthenticationRequired(true)
                    .setInvalidatedByBiometricEnrollment(true);
            if (Build.VERSION.SDK_INT >= 30) {
                spec.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG);
            }
            KeyPairGenerator generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_RSA, "AndroidKeyStore");
            generator.initialize(spec.build());
            PublicKey held = generator.generateKeyPair().getPublic();
            // A copy outside the keystore: encrypting with the keystore's own handle would ask for authentication.
            PublicKey open = KeyFactory.getInstance(held.getAlgorithm()).generatePublic(new X509EncodedKeySpec(held.getEncoded()));
            Cipher cipher = Cipher.getInstance(TRANSFORMATION);
            cipher.init(Cipher.ENCRYPT_MODE, open, oaep());
            return cipher.doFinal(key);
        } catch (Exception e) {
            return null;
        }
    }

    /** Forgets the keystore key. */
    public static void forget() {
        try {
            KeyStore store = KeyStore.getInstance("AndroidKeyStore");
            store.load(null);
            store.deleteEntry(ALIAS);
        } catch (Exception ignored) {
        }
    }

    /** Asks for the biometric and unwraps `sealed`; `result(token, …)` gets the answer. */
    public static void open(final byte[] sealed, final String title, final String cancel, final long token) {
        MAIN.post(new Runnable() {
            @Override
            public void run() {
                Activity activity = Bridge.front();
                if (Build.VERSION.SDK_INT < 28 || activity == null) {
                    result(token, UNAVAILABLE, null);
                    return;
                }
                final Cipher cipher;
                try {
                    KeyStore store = KeyStore.getInstance("AndroidKeyStore");
                    store.load(null);
                    PrivateKey key = (PrivateKey) store.getKey(ALIAS, null);
                    if (key == null) {
                        result(token, STALE, null);
                        return;
                    }
                    cipher = Cipher.getInstance(TRANSFORMATION);
                    cipher.init(Cipher.DECRYPT_MODE, key, oaep());
                } catch (KeyPermanentlyInvalidatedException e) {
                    result(token, STALE, null);
                    return;
                } catch (Exception e) {
                    result(token, STALE, null);
                    return;
                }
                Executor executor = new Executor() {
                    @Override
                    public void execute(Runnable command) {
                        MAIN.post(command);
                    }
                };
                BiometricPrompt prompt = new BiometricPrompt.Builder(activity)
                        .setTitle(title)
                        .setNegativeButton(cancel, executor, new DialogInterface.OnClickListener() {
                            @Override
                            public void onClick(DialogInterface dialog, int which) {
                                result(token, FALLBACK, null);
                            }
                        })
                        .build();
                prompt.authenticate(new BiometricPrompt.CryptoObject(cipher), new CancellationSignal(), executor,
                        new BiometricPrompt.AuthenticationCallback() {
                            @Override
                            public void onAuthenticationSucceeded(BiometricPrompt.AuthenticationResult answer) {
                                try {
                                    result(token, OK, answer.getCryptoObject().getCipher().doFinal(sealed));
                                } catch (Exception e) {
                                    result(token, STALE, null);
                                }
                            }

                            @Override
                            public void onAuthenticationError(int code, CharSequence message) {
                                if (code == BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED || code == BiometricPrompt.BIOMETRIC_ERROR_CANCELED) {
                                    result(token, CANCELLED, null);
                                } else if (code == BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT
                                        || code == BiometricPrompt.BIOMETRIC_ERROR_LOCKOUT_PERMANENT) {
                                    result(token, LOCKOUT, null);
                                } else if (code == BiometricPrompt.BIOMETRIC_ERROR_HW_NOT_PRESENT
                                        || code == BiometricPrompt.BIOMETRIC_ERROR_HW_UNAVAILABLE
                                        || code == BiometricPrompt.BIOMETRIC_ERROR_NO_BIOMETRICS) {
                                    result(token, UNAVAILABLE, null);
                                } else {
                                    result(token, FAILED, null);
                                }
                            }
                        });
            }
        });
    }
}
