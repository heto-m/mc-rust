package dev.mcr.bridge.gpu;

import com.mojang.renderpearl.api.pipeline.BindGroupLayout;
import com.mojang.renderpearl.api.pipeline.BlendEquation;
import com.mojang.renderpearl.api.pipeline.BlendFunction;
import com.mojang.renderpearl.api.pipeline.ColorTargetState;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import com.mojang.renderpearl.api.pipeline.ShaderType;
import com.mojang.renderpearl.backend.api.BackendRenderPipeline;
import dev.mcr.bridge.BridgeAbi;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/** Flattens a pipeline's CreateInfo into the byte layout `mcr-modbridge::gpu::pipeline` parses. */
public final class PipelineInfo {
    public static final String DUMP_PROPERTY = "mcr.gpu.dump";

    private ByteBuffer out = ByteBuffer.allocate(1 << 16).order(ByteOrder.nativeOrder());

    public static ByteBuffer encode(BackendRenderPipeline.CreateInfo info) {
        PipelineInfo w = new PipelineInfo();
        w.write(info);
        return w.out.flip();
    }

    private void ensure(int bytes) {
        if (out.remaining() < bytes) {
            ByteBuffer bigger = ByteBuffer.allocate(Math.max(out.capacity() * 2, out.position() + bytes)).order(ByteOrder.nativeOrder());
            bigger.put(out.flip());
            out = bigger;
        }
    }

    private void u32(int v) {
        ensure(4);
        out.putInt(v);
    }

    private void f32(float v) {
        ensure(4);
        out.putFloat(v);
    }

    private void bytes(ByteBuffer data) {
        int len = data.remaining();
        u32(len);
        ensure(len + 3);
        out.put(data.duplicate());
        while ((out.position() & 3) != 0) {
            out.put((byte) 0);
        }
    }

    private void string(String s) {
        bytes(ByteBuffer.wrap(s.getBytes(StandardCharsets.UTF_8)));
    }

    private void blend(BlendEquation e) {
        u32(e.sourceFactor().ordinal());
        u32(e.destFactor().ordinal());
        u32(e.op().ordinal());
    }

    private void write(BackendRenderPipeline.CreateInfo info) {
        u32(BridgeAbi.PIPELINE_MAGIC);
        u32(BridgeAbi.PIPELINE_FORMAT);
        string(info.name());
        u32(info.shaders().size());
        for (BackendRenderPipeline.CreateInfo.Shader shader : info.shaders()) {
            u32(shader.module().type() == ShaderType.VERTEX ? 0 : 1);
            bytes(shader.module().spv());
        }
        u32(info.vertexBuffers().size());
        for (BackendRenderPipeline.CreateInfo.VertexBuffer vb : info.vertexBuffers()) {
            u32(vb.bufferSlot());
            u32(vb.stride());
            u32(vb.stepRate());
        }
        u32(info.attribBindings().size());
        for (BackendRenderPipeline.CreateInfo.AttribBinding a : info.attribBindings()) {
            u32(a.bufferSlot());
            u32(a.location());
            u32(a.offset());
            u32(a.format().ordinal());
        }
        u32(info.uniforms().size());
        for (BindGroupLayout.UniformDescription u : info.uniforms()) {
            u32(u.type().ordinal());
            u32(u.gpuFormat() == null ? -1 : u.gpuFormat().ordinal());
            string(u.name());
        }
        u32(info.pushConstantsSize());
        DepthStencilState depth = info.depthStencilState();
        if (depth == null) {
            u32(0);
            u32(0);
            u32(0);
            f32(0);
            f32(0);
        } else {
            u32(1);
            u32(depth.depthTest().ordinal());
            u32(depth.writeDepth() ? 1 : 0);
            f32(depth.depthBiasScaleFactor());
            f32(depth.depthBiasConstant());
        }
        u32(info.polygonMode().ordinal());
        u32(info.cull() ? 1 : 0);
        u32(info.primitiveTopology().ordinal());
        u32(info.colorTargetStates().size());
        for (ColorTargetState target : info.colorTargetStates()) {
            if (target == null) {
                u32(0);
                u32(0);
                u32(0);
                u32(0);
                for (int i = 0; i < 6; i++) {
                    u32(0);
                }
                continue;
            }
            u32(1);
            u32(target.format().ordinal());
            u32(target.writeMask());
            BlendFunction blend = target.blendFunction().orElse(null);
            u32(blend == null ? 0 : 1);
            if (blend == null) {
                for (int i = 0; i < 6; i++) {
                    u32(0);
                }
            } else {
                blend(blend.color());
                blend(blend.alpha());
            }
        }
    }

    /** Test aid: keeps every CreateInfo the frontend produced, for the Rust shader tests. */
    public static void dump(BackendRenderPipeline.CreateInfo info, ByteBuffer encoded) {
        String dir = System.getProperty(DUMP_PROPERTY);
        if (dir == null) {
            return;
        }
        String file = info.name().replaceAll("[^A-Za-z0-9._-]", "_") + ".mcrp";
        try {
            Path path = Path.of(dir);
            Files.createDirectories(path);
            byte[] bytes = new byte[encoded.remaining()];
            encoded.duplicate().get(bytes);
            Files.write(path.resolve(file), bytes);
        } catch (IOException e) {
            throw new IllegalStateException("dumping " + info.name(), e);
        }
    }
}
