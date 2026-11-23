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
 * is written anywhere. Rust asks for the camera permission first; the camera opens once it is asked for
 * ({@link #open}) and the preview's surface is there, whichever comes last.
 */
public final class Camera {
    /** {@link #state}: opening, frames arriving, or the camera could not be opened or was lost. */
    public static final int OPENING = 0;
    public static final int RUNNING = 1;
    public static final int FAILED = 2;

    private static final Object LOCK = new Object();
    private static HandlerThread thread;
    private static CameraDevice device;
    private static CameraCaptureSession session;
    private static ImageReader reader;
    private static volatile int state = FAILED;
    private static volatile int width;
    private static volatile int height;
    private static int orientation;
    private static byte[] luma;
    /** The preview's view, its buffers' size, and whether Rust asked for the camera. UI thread. */
    private static TextureView texture;
    private static Size previewSize;
    private static boolean wanted;

    private Camera() {}

    /** The preview's view for the page's native component. UI thread. */
    public static View view(Context context) {
        final TextureView view = new TextureView(context);
        view.setSurfaceTextureListener(new TextureView.SurfaceTextureListener() {
            @Override
            public void onSurfaceTextureAvailable(SurfaceTexture surface, int w, int h) {
                if (wanted && device == null) {
                    openCamera();
                }
                fill(view);
            }

            @Override
            public void onSurfaceTextureSizeChanged(SurfaceTexture surface, int w, int h) {
                fill(view);
            }

            @Override
            public boolean onSurfaceTextureDestroyed(SurfaceTexture surface) {
                return true;
            }

            @Override
            public void onSurfaceTextureUpdated(SurfaceTexture surface) {}
        });
        texture = view;
        return view;
    }

    /**
     * Opens the back camera (or the first one) once the preview's surface is there. Whether it is opening or waiting
     * for the surface; {@link #state} tells how that goes. Any thread.
     */
    public static boolean open() {
        close();
        wanted = true;
        state = OPENING;
        final TextureView view = texture;
        if (view != null) {
            view.post(new Runnable() {
                @Override
                public void run() {
                    if (wanted && device == null && view.isAvailable()) {
                        openCamera();
                    }
                }
            });
        }
        return true;
    }

    private static void openCamera() {
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
            thread = new HandlerThread("safay-camera");
            thread.start();
            final Handler handler = new Handler(thread.getLooper());
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
            manager.openCamera(id, new CameraDevice.StateCallback() {
                @Override
                public void onOpened(CameraDevice camera) {
                    device = camera;
                    start(camera, handler);
                }

                @Override
                public void onDisconnected(CameraDevice camera) {
                    camera.close();
                    state = FAILED;
                }

                @Override
                public void onError(CameraDevice camera, int error) {
                    camera.close();
                    state = FAILED;
                }
            }, handler);
        } catch (CameraAccessException | SecurityException | IllegalArgumentException e) {
            close();
        }
    }

    /** Stops the camera and lets its frames go. Any thread. */
    public static void close() {
        wanted = false;
        state = FAILED;
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
        if (thread != null) {
            thread.quitSafely();
            thread = null;
        }
        synchronized (LOCK) {
            luma = null;
        }
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
     * buffers upright and stretches them over the view, which this scales back to the buffers' own proportions.
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

    private static void start(final CameraDevice camera, final Handler handler) {
        if (reader == null) {
            return;
        }
        final Surface frames = reader.getSurface();
        final List<Surface> outputs = new ArrayList<>();
        outputs.add(frames);
        final TextureView view = texture;
        if (view != null && view.isAvailable() && previewSize != null) {
            SurfaceTexture surfaceTexture = view.getSurfaceTexture();
            surfaceTexture.setDefaultBufferSize(previewSize.getWidth(), previewSize.getHeight());
            outputs.add(new Surface(surfaceTexture));
            view.post(new Runnable() {
                @Override
                public void run() {
                    fill(view);
                }
            });
        }
        try {
            camera.createCaptureSession(outputs, new CameraCaptureSession.StateCallback() {
                @Override
                public void onConfigured(CameraCaptureSession s) {
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
                    state = FAILED;
                }
            }, handler);
        } catch (CameraAccessException | IllegalStateException e) {
            state = FAILED;
        }
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
