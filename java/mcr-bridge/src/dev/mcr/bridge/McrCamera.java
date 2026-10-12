package dev.mcr.bridge;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import org.joml.Matrix4f;
import org.joml.Matrix4fc;
import org.jspecify.annotations.Nullable;

/** mcr's camera from `FrameBegin`: Java renders the world from Rust's eye with Rust's projection. */
public final class McrCamera {
    private static final ValueLayout.OfDouble DOUBLE = ValueLayout.JAVA_DOUBLE_UNALIGNED;
    private static final ValueLayout.OfFloat FLOAT = ValueLayout.JAVA_FLOAT_UNALIGNED;
    private static final double MAX_COORD = 3.0E7;

    private double x;
    private double y;
    private double z;
    private float yRot;
    private float xRot;
    private boolean detached;
    private boolean valid;
    private final Matrix4f view = new Matrix4f();
    private final Matrix4f projection = new Matrix4f();
    private final float[] scratch = new float[16];

    void read(MemorySegment p, boolean detached) {
        x = p.get(DOUBLE, BridgeAbi.FRAME_CAMERA);
        y = p.get(DOUBLE, BridgeAbi.FRAME_CAMERA + 8L);
        z = p.get(DOUBLE, BridgeAbi.FRAME_CAMERA + 16L);
        float yaw = p.get(FLOAT, BridgeAbi.FRAME_YAW);
        float pitch = p.get(FLOAT, BridgeAbi.FRAME_PITCH);
        boolean ok = coord(x) && coord(y) && coord(z) && Float.isFinite(yaw) && Float.isFinite(pitch);
        ok &= matrix(p, BridgeAbi.FRAME_VIEW, view);
        ok &= matrix(p, BridgeAbi.FRAME_PROJ, projection);
        // mcr: yaw 0 looks toward -Z and turns left, pitch up is positive; vanilla's are the reverse.
        yRot = 180.0F - (float) Math.toDegrees(yaw);
        xRot = Math.clamp(-(float) Math.toDegrees(pitch), -90.0F, 90.0F);
        this.detached = detached;
        valid = ok;
    }

    private static boolean coord(double v) {
        return Double.isFinite(v) && Math.abs(v) <= MAX_COORD;
    }

    private boolean matrix(MemorySegment p, int offset, Matrix4f out) {
        for (int i = 0; i < 16; i++) {
            float v = p.get(FLOAT, offset + 4L * i);
            if (!Float.isFinite(v)) {
                return false;
            }
            scratch[i] = v;
        }
        out.set(scratch);
        return true;
    }

    boolean valid() {
        return valid;
    }

    public double x() {
        return x;
    }

    public double y() {
        return y;
    }

    public double z() {
        return z;
    }

    public float yRot() {
        return yRot;
    }

    public float xRot() {
        return xRot;
    }

    public boolean detached() {
        return detached;
    }

    /**
     * A conventional perspective with Rust's field of view and aspect for vanilla's culling frustum, or
     * null to keep vanilla's; Rust's reverse-Z projection has no far plane, which never-ending frustum
     * walks such as `offsetToFullyIncludeCameraCube` cannot take.
     */
    public @Nullable Matrix4f cullProjection(float near, float far, boolean zeroToOne) {
        float sx = Math.abs(projection.m00());
        float sy = Math.abs(projection.m11());
        if (!(sx > 1.0E-3F && sy > 1.0E-3F && sx < 1.0E3F && sy < 1.0E3F && far > near)) {
            return null;
        }
        float fovY = 2.0F * (float) Math.atan(1.0 / sy);
        return new Matrix4f().perspective(fovY, sy / sx, near, far, zeroToOne);
    }

    /**
     * The level projection for the world slots: Rust's projection with Rust's view roll, so it lands on
     * mcr's own pixels and depth once combined with Java's yaw/pitch view rotation.
     */
    public Matrix4f levelProjection(Matrix4fc javaViewRotation) {
        Matrix4f inverse = new Matrix4f(javaViewRotation).transpose();
        return flipped(new Matrix4f(projection)).mul(view).mul(inverse);
    }

    /** naga flips clip-space Y for Java's GL-style shaders; flipping the projection too draws top-down. */
    public static Matrix4f flipped(Matrix4f projection) {
        return projection.scaleLocal(1.0F, -1.0F, 1.0F);
    }
}
