package dev.gpui.mobile;

import android.content.Context;
import android.util.AttributeSet;
import android.view.Choreographer;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;

/**
 * Sample host for foreign-parent GPUI editors on Android.
 *
 * <p>Embeds one GPUI editor in a {@link SurfaceView}: the surface crosses to
 * native code once, frames are pumped from the {@link Choreographer}, and
 * resizes, focus, touches, and keys are forwarded through the
 * {@code native*} entry points (see {@code android/foreign_parent.rs}).
 * The host activity must load its own native library (which links
 * {@code gpui-android} as a {@code cdylib}) before instantiating this view.
 */
public class GpuiForeign extends SurfaceView
        implements SurfaceHolder.Callback, Choreographer.FrameCallback {

    private static native long nativeCreate(Surface surface, float scale, String label);

    private static native void nativeDestroy(long editorId);

    private static native void nativeRequestFrame(long editorId);

    private static native void nativeResize(long editorId, int width, int height, float scale);

    private static native void nativeSetActive(long editorId, boolean isActive);

    private static native void nativeTouch(
            long editorId,
            int pointerId,
            int action,
            float x,
            float y,
            long timestampNs,
            int tool,
            float pressure);

    private static native void nativeKey(
            long editorId, int keyCode, int action, int metaState, int unicodeChar);

    private static native void nativeSurfaceCreated(long editorId, Surface surface);

    private static native void nativeSurfaceDestroyed(long editorId);

    private static native long nativeDroppedFrames(long editorId);

    private static native long nativeCoalescedFrames(long editorId);

    private final String label;
    private long editorId;
    private boolean frameScheduled;

    public GpuiForeign(Context context, String label) {
        super(context);
        this.label = label;
        init();
    }

    public GpuiForeign(Context context, AttributeSet attrs, String label) {
        super(context, attrs);
        this.label = label;
        init();
    }

    private void init() {
        setFocusable(true);
        setFocusableInTouchMode(true);
        getHolder().addCallback(this);
    }

    private float densityScale() {
        return getResources().getDisplayMetrics().density;
    }

    private int toolCode(int androidToolType) {
        // Contract: 0 finger, 1 stylus (including eraser), else compatibility.
        if (androidToolType == MotionEvent.TOOL_TYPE_FINGER) {
            return 0;
        }
        if (androidToolType == MotionEvent.TOOL_TYPE_STYLUS
                || androidToolType == MotionEvent.TOOL_TYPE_ERASER) {
            return 1;
        }
        return 2;
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        if (editorId == 0) {
            editorId = nativeCreate(holder.getSurface(), densityScale(), label);
        } else {
            nativeSurfaceCreated(editorId, holder.getSurface());
        }
        scheduleFrame();
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        if (editorId != 0) {
            nativeResize(editorId, width, height, densityScale());
        }
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        if (editorId != 0) {
            nativeSurfaceDestroyed(editorId);
        }
        frameScheduled = false;
        Choreographer.getInstance().removeFrameCallback(this);
    }

    /** Release the editor. Call from the host's destroy path. */
    public void destroy() {
        frameScheduled = false;
        Choreographer.getInstance().removeFrameCallback(this);
        getHolder().removeCallback(this);
        if (editorId != 0) {
            nativeDestroy(editorId);
            editorId = 0;
        }
    }

    private void scheduleFrame() {
        if (editorId != 0 && !frameScheduled) {
            frameScheduled = true;
            Choreographer.getInstance().postFrameCallback(this);
        }
    }

    @Override
    public void doFrame(long frameTimeNanos) {
        frameScheduled = false;
        if (editorId == 0) {
            return;
        }
        nativeRequestFrame(editorId);
        scheduleFrame();
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        if (editorId == 0) {
            return false;
        }
        int action = event.getActionMasked();
        int changed = event.getActionIndex();
        for (int index = 0; index < event.getPointerCount(); index++) {
            int pointerAction = action;
            if ((action == MotionEvent.ACTION_POINTER_DOWN
                            || action == MotionEvent.ACTION_POINTER_UP)
                    && index != changed) {
                // Non-changing pointers in a multi-touch event report as moves.
                pointerAction = MotionEvent.ACTION_MOVE;
            }
            nativeTouch(
                    editorId,
                    event.getPointerId(index),
                    pointerAction,
                    event.getX(index),
                    event.getY(index),
                    event.getEventTimeNanos(),
                    toolCode(event.getToolType(index)),
                    event.getPressure(index));
        }
        return true;
    }

    private boolean forwardKey(KeyEvent event) {
        if (editorId == 0) {
            return false;
        }
        // Contract: 0 down, 1 up; repeats arrive as repeated downs.
        int action = event.getAction() == KeyEvent.ACTION_UP ? 1 : 0;
        nativeKey(
                editorId,
                event.getKeyCode(),
                action,
                event.getMetaState(),
                event.getUnicodeChar());
        return true;
    }

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        return forwardKey(event) || super.onKeyDown(keyCode, event);
    }

    @Override
    public boolean onKeyUp(int keyCode, KeyEvent event) {
        return forwardKey(event) || super.onKeyUp(keyCode, event);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (editorId != 0) {
            nativeSetActive(editorId, hasFocus);
        }
    }

    /** Realtime health counters for the host (see the embedding contract). */
    public long[] frameStats() {
        if (editorId == 0) {
            return new long[] {0, 0};
        }
        return new long[] {nativeDroppedFrames(editorId), nativeCoalescedFrames(editorId)};
    }
}
