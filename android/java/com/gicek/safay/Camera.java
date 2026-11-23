package com.gicek.safay;

import android.content.Context;
import android.graphics.ImageFormat;
import android.graphics.Matrix;
import android.graphics.SurfaceTexture;
import android.hardware.camera2.CameraAccessException;
import android.hardware.camera2.CameraCaptureSession;
import android.hardware.camera2.CameraCharacteristics;
import android.hardware.camera2.CameraDevice;
import android.hardware.camera2.CameraManager;
import android.hardware.camera2.CaptureRequest;
import android.hardware.camera2.params.StreamConfigurationMap;
import android.media.Image;
import android.media.ImageReader;
import android.os.Handler;
import android.os.HandlerThread;
import android.util.Size;
import android.view.Surface;
import android.view.TextureView;
import android.view.View;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * The back camera for scanning QR codes (src/platform/camera_android.rs), through Camera2, to two places at once: the
 * preview, straight into the {@link TextureView} of the page's native component ({@link #view}), drawn by Android as a
 * camera app's is; and frames of about 1280 by 720 in YUV, whose latest luma Rust takes to read QR codes from. Nothing
 * is written anywhere. Rust asks for the camera permission first.
 *
 * <p>The camera's state is touched on one thread only, the camera thread: {@link #open}, {@link #close}, the preview's
 * surface and the app going to the background or coming back are posted to it, and Camera2 calls back on it. Each open
 * and close starts a new generation; a callback of an earlier one closes what it opened instead of keeping it. The
 * camera opens once it is wanted, the preview's surface is there and the app is in front, whichever comes last, and
 * closes while the app is in the background.
 */
public final class Camera {
    /** {@link #state}: opening (or waiting to), frames arriving, or the camera could not be opened or was lost. */
    public static final int OPENING = 0;
    public static final int RUNNING = 1;
    public static final int FAILED = 2;

    private static final Object LOCK = new Object();
    /** The camera thread, started the first time the camera is used. */
    private static volatile Handler handler;

    // Read from any thread.
    private static volatile int state = FAILED;
    private static volatile int width;
    private static volatile int height;
    /** The latest frame's luma, until Rust takes it. Under LOCK. */
    private static byte[] luma;

    /** The preview's view: set on the UI thread, read on the camera thread. */
    private static volatile TextureView texture;

    // The camera thread's own.
    private static int generation;
    private static boolean wanted;
    private static boolean background;
    private static boolean opening;
    private static CameraDevice device;
    private static CameraCaptureSession session;
    private static ImageReader reader;
    private static Surface preview;
    private static Size previewSize;
    private static int orientation;

    private Camera() {}

    private static synchronized Handler thread() {
        if (handler == null) {
            HandlerThread thread = new HandlerThread("safay-camera");
            thread.start();
            handler = new Handler(thread.getLooper());
        }
        return handler;
    }

    /** Runs `task` on the camera thread, if it was ever started. */
    private static void post(Runnable task) {
        Handler h = handler;
        if (h != null) {
            h.post(task);
        }
    }

    /** The preview's view for the page's native component. UI thread. */
    public static View view(Context context) {
        final TextureView view = new TextureView(context);
        view.setSurfaceTextureListener(new TextureView.SurfaceTextureListener() {
            @Override
            public void onSurfaceTextureAvailable(SurfaceTexture surface, int w, int h) {
                thread().post(new Runnable() {
                    @Override
                    public void run() {
                        openIfReady();
                    }
                });
                fill(view);
            }

            @Override
            public void onSurfaceTextureSizeChanged(SurfaceTexture surface, int w, int h) {
                fill(view);
            }

            @Override
            public boolean onSurfaceTextureDestroyed(SurfaceTexture surface) {
                // The preview has nowhere to go: the camera closes until a surface is there again.
                thread().post(new Runnable() {
                    @Override
                    public void run() {
                        if (texture == view) {
                            texture = null;
                        }
                        release();
                        if (wanted) {
                            state = OPENING;
                        }
                    }
                });
                return true;
            }

            @Override
            public void onSurfaceTextureUpdated(SurfaceTexture surface) {}
        });
        texture = view;
        return view;
    }

    /** The page's native component is gone: its view is let go. UI thread. */
    public static void forget(View view) {
        if (texture == view) {
            texture = null;
        }
    }

    /**
     * Opens the back camera (or the first one) as soon as it can; {@link #state} tells how that goes. Returns once the
     * camera thread has taken it, so a close asked for earlier cannot land after it. Any thread but the main one.
     */
    public static void open() {
        final java.util.concurrent.CountDownLatch taken = new java.util.concurrent.CountDownLatch(1);
        thread().post(new Runnable() {
            @Override
            public void run() {
                generation++;
                release();
                wanted = true;
                state = OPENING;
                taken.countDown();
                openIfReady();
            }
        });
        try {
            taken.await(2, java.util.concurrent.TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    /** Stops the camera and lets its frames go. Any thread. */
    public static void close() {
        synchronized (LOCK) {
            luma = null;
        }
        post(new Runnable() {
            @Override
            public void run() {
                generation++;
                wanted = false;
                release();
                state = FAILED;
            }
        });
    }

    /** The app went to the background: the camera closes, to open again when it comes back. Main thread. */
    static void paused() {
        post(new Runnable() {
            @Override
            public void run() {
                background = true;
                if (device != null || opening) {
                    generation++;
                    release();
                }
                if (wanted) {
                    state = OPENING;
                }
            }
        });
    }

    /** The app is in front again. Main thread. */
    static void resumed() {
        post(new Runnable() {
            @Override
            public void run() {
                background = false;
                openIfReady();
            }
        });
    }

    public static int state() {
        return state;
    }

    /** The frames' size, as the sensor sees them. */
    public static int width() {
        return width;
    }

    public static int height() {
        return height;
    }

    /** The latest frame's luma since the last call, width by height bytes, or null. */
    public static byte[] takeLuma() {
        synchronized (LOCK) {
            byte[] taken = luma;
            luma = null;
            return taken;
        }
    }

    // ---- The camera thread ----------------------------------------------------------------------------------------

    private static void openIfReady() {
        TextureView view = texture;
        if (!wanted || background || opening || device != null || view == null || !view.isAvailable()) {
            return;
        }
        Context context = Bridge.application();
        if (context == null) {
            state = FAILED;
            return;
        }
        CameraManager manager = (CameraManager) context.getSystemService(Context.CAMERA_SERVICE);
        try {
            String id = backCamera(manager);
            if (id == null) {
                state = FAILED;
                return;
            }
            CameraCharacteristics traits = manager.getCameraCharacteristics(id);
            Integer sensor = traits.get(CameraCharacteristics.SENSOR_ORIENTATION);
            orientation = sensor == null ? 0 : sensor;
            StreamConfigurationMap map = traits.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP);
            Size size = map == null ? new Size(1280, 720) : size(map.getOutputSizes(ImageFormat.YUV_420_888));
            previewSize = map == null ? new Size(1920, 1080) : previewSize(map.getOutputSizes(SurfaceTexture.class));
            width = size.getWidth();
            height = size.getHeight();
            reader = ImageReader.newInstance(width, height, ImageFormat.YUV_420_888, 2);
            reader.setOnImageAvailableListener(new ImageReader.OnImageAvailableListener() {
                @Override
                public void onImageAvailable(ImageReader r) {
                    Image image = r.acquireLatestImage();
                    if (image == null) {
                        return;
                    }
                    try {
                        frame(image);
                    } finally {
                        image.close();
                    }
                }
            }, handler);
            final int mine = generation;
            opening = true;
            manager.openCamera(id, new CameraDevice.StateCallback() {
                @Override
                public void onOpened(CameraDevice camera) {
                    if (mine != generation) {
                        camera.close();
                        return;
                    }
                    opening = false;
                    device = camera;
                    startSession(camera, mine);
                }

                @Override
                public void onDisconnected(CameraDevice camera) {
                    lost(camera, mine);
                }

                @Override
                public void onError(CameraDevice camera, int error) {
                    lost(camera, mine);
                }
            }, handler);
        } catch (CameraAccessException | SecurityException | IllegalArgumentException e) {
            release();
            state = FAILED;
        }
    }

    /** The camera went away: taken by another app, or by the system while this one is in the background. */
    private static void lost(CameraDevice camera, int mine) {
        camera.close();
        if (mine != generation) {
            return;
        }
        device = null;
        release();
        // In the background it opens again on coming back; in front, it is gone.
        state = background && wanted ? OPENING : FAILED;
    }

    private static void startSession(final CameraDevice camera, final int mine) {
        TextureView view = texture;
        if (reader == null || view == null || !view.isAvailable() || previewSize == null) {
            release();
            state = wanted ? OPENING : FAILED;
            return;
        }
        SurfaceTexture surfaceTexture = view.getSurfaceTexture();
        if (surfaceTexture == null) {
            release();
            state = OPENING;
            return;
        }
        surfaceTexture.setDefaultBufferSize(previewSize.getWidth(), previewSize.getHeight());
        preview = new Surface(surfaceTexture);
        final List<Surface> outputs = new ArrayList<>();
        outputs.add(reader.getSurface());
        outputs.add(preview);
        final TextureView shown = view;
        view.post(new Runnable() {
            @Override
            public void run() {
                fill(shown);
            }
        });
        try {
            camera.createCaptureSession(outputs, new CameraCaptureSession.StateCallback() {
                @Override
                public void onConfigured(CameraCaptureSession s) {
                    if (mine != generation) {
                        s.close();
                        return;
                    }
                    session = s;
                    try {
                        CaptureRequest.Builder request = camera.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW);
                        for (Surface output : outputs) {
                            request.addTarget(output);
                        }
                        request.set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE);
                        s.setRepeatingRequest(request.build(), null, handler);
                        state = RUNNING;
                    } catch (CameraAccessException | IllegalStateException e) {
                        state = FAILED;
                    }
                }

                @Override
                public void onConfigureFailed(CameraCaptureSession s) {
                    if (mine == generation) {
                        state = FAILED;
                    }
                }
            }, handler);
        } catch (CameraAccessException | IllegalStateException e) {
            state = FAILED;
        }
    }

    /** Closes the session, the camera, the frames and the preview's surface, whatever is open. */
    private static void release() {
        opening = false;
        if (session != null) {
            session.close();
            session = null;
        }
        if (device != null) {
            device.close();
            device = null;
        }
        if (reader != null) {
            reader.close();
            reader = null;
        }
        if (preview != null) {
            preview.release();
            preview = null;
        }
    }

    private static void frame(Image image) {
        int w = image.getWidth();
        int h = image.getHeight();
        byte[] y = new byte[w * h];
        copy(image.getPlanes()[0], w, h, y);
        synchronized (LOCK) {
            luma = y;
        }
    }

    /**
     * Fills the view with the preview, centered and cut to its shape rather than stretched: Android turns the camera's
     * buffers upright and stretches them over the view, which this scales back to the buffers' own proportions. UI
     * thread.
     */
    private static void fill(TextureView view) {
        Size size = previewSize;
        int vw = view.getWidth();
        int vh = view.getHeight();
        if (size == null || vw == 0 || vh == 0) {
            return;
        }
        boolean turned = orientation % 180 != 0;
        float cw = turned ? size.getHeight() : size.getWidth();
        float ch = turned ? size.getWidth() : size.getHeight();
        float scale = Math.max(vw / cw, vh / ch);
        Matrix matrix = new Matrix();
        matrix.setScale(cw * scale / vw, ch * scale / vh, vw / 2f, vh / 2f);
        view.setTransform(matrix);
    }

    private static String backCamera(CameraManager manager) throws CameraAccessException {
        String first = null;
        for (String id : manager.getCameraIdList()) {
            if (first == null) {
                first = id;
            }
            Integer facing = manager.getCameraCharacteristics(id).get(CameraCharacteristics.LENS_FACING);
            if (facing != null && facing == CameraCharacteristics.LENS_FACING_BACK) {
                return id;
            }
        }
        return first;
    }

    /** The largest 16:9 size up to 1920 by 1080, as sharp as the screen needs; else the largest up to that. */
    private static Size previewSize(Size[] sizes) {
        if (sizes == null || sizes.length == 0) {
            return new Size(1920, 1080);
        }
        Size wide = null;
        Size any = null;
        for (Size s : sizes) {
            int w = s.getWidth();
            int h = s.getHeight();
            if (w > 1920 || h > 1080) {
                continue;
            }
            if (w * 9 == h * 16 && (wide == null || w * h > wide.getWidth() * wide.getHeight())) {
                wide = s;
            }
            if (any == null || w * h > any.getWidth() * any.getHeight()) {
                any = s;
            }
        }
        return wide != null ? wide : (any != null ? any : sizes[0]);
    }

    /**
     * 1280 by 720 where the camera has it (wide as a phone's screen, sharp enough to fill it); else the smallest 16:9
     * size at least 720 high, else the smallest size at least 640 wide, else the largest.
     */
    private static Size size(Size[] sizes) {
        if (sizes == null || sizes.length == 0) {
            return new Size(1280, 720);
        }
        Size wide = null;
        Size any = null;
        Size largest = sizes[0];
        for (Size s : sizes) {
            int w = s.getWidth();
            int h = s.getHeight();
            if (w == 1280 && h == 720) {
                return s;
            }
            if (w * h > largest.getWidth() * largest.getHeight()) {
                largest = s;
            }
            if (w * 9 == h * 16 && h >= 720 && h <= 1080 && (wide == null || w * h < wide.getWidth() * wide.getHeight())) {
                wide = s;
            }
            if (w >= 640 && w <= 1920 && (any == null || w * h < any.getWidth() * any.getHeight())) {
                any = s;
            }
        }
        return wide != null ? wide : (any != null ? any : largest);
    }

    /** The plane's first w by h samples, whatever its strides: whole rows at once where they are packed. */
    private static void copy(Image.Plane plane, int w, int h, byte[] out) {
        ByteBuffer buffer = plane.getBuffer();
        int row = plane.getRowStride();
        int pixel = plane.getPixelStride();
        int limit = buffer.limit();
        for (int line = 0; line < h; line++) {
            int base = line * row;
            if (pixel == 1 && base + w <= limit) {
                buffer.position(base);
                buffer.get(out, line * w, w);
                continue;
            }
            for (int x = 0; x < w; x++) {
                int index = base + x * pixel;
                out[line * w + x] = index < limit ? buffer.get(index) : 0;
            }
        }
    }
}
