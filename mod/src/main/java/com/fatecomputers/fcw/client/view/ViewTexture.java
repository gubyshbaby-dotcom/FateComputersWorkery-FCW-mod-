package com.fatecomputers.fcw.client.view;

import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.resources.ResourceLocation;
import org.lwjgl.opengl.GL;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL33;
import org.lwjgl.system.MemoryUtil;

import javax.annotation.Nullable;
import java.nio.ByteBuffer;

/**
 * GPU texture of one guest screen. The native library writes changed pixels straight into the
 * memory of the {@link NativeImage} (via a direct ByteBuffer over it), and only the changed
 * rectangles are uploaded with {@link NativeImage#upload}.
 */
public final class ViewTexture implements AutoCloseable {
    private final ResourceLocation id;
    private int width;
    private int height;
    @Nullable
    private NativeImage image;
    @Nullable
    private DynamicTexture texture;
    @Nullable
    private ByteBuffer buffer;
    private boolean hasFrame;

    public ViewTexture(ResourceLocation id) {
        this.id = id;
    }

    public ResourceLocation id() {
        return id;
    }

    public int width() {
        return width;
    }

    public int height() {
        return height;
    }

    public boolean hasFrame() {
        return hasFrame && texture != null;
    }

    @Nullable
    public ByteBuffer buffer() {
        return buffer;
    }

    /** (Re)allocates the texture; returns true when it was recreated (needs a full frame). */
    public boolean ensureSize(int w, int h) {
        if (texture != null && w == width && h == height) {
            return false;
        }
        NativeImage img = new NativeImage(NativeImage.Format.RGBA, w, h, true);
        DynamicTexture tex = new DynamicTexture(img);
        // Registering under the same id closes the previous texture and its image.
        Minecraft.getInstance().getTextureManager().register(id, tex);
        image = img;
        texture = tex;
        width = w;
        height = h;
        buffer = MemoryUtil.memByteBuffer(img.pixels, w * h * 4);
        hasFrame = false;
        tex.bind();
        if (GL.getCapabilities().OpenGL33) {
            // Guest pixels are opaque: ignore whatever alpha the decoder leaves in the buffer.
            GL11.glTexParameteri(GL11.GL_TEXTURE_2D, GL33.GL_TEXTURE_SWIZZLE_A, GL11.GL_ONE);
        }
        return true;
    }

    /** Uploads rectangles {x,y,w,h}*count from the buffer; count -1 uploads everything. */
    public void upload(int[] rects, int count) {
        if (texture == null || image == null) {
            return;
        }
        texture.bind();
        if (count < 0) {
            image.upload(0, 0, 0, 0, 0, width, height, true, false, false, false);
        } else {
            for (int i = 0; i < count; i++) {
                int x = rects[i * 4];
                int y = rects[i * 4 + 1];
                int w = rects[i * 4 + 2];
                int h = rects[i * 4 + 3];
                if (w <= 0 || h <= 0 || x < 0 || y < 0 || x + w > width || y + h > height) {
                    continue;
                }
                image.upload(0, x, y, x, y, w, h, true, false, false, false);
            }
        }
        hasFrame = true;
    }

    @Override
    public void close() {
        if (texture != null) {
            Minecraft.getInstance().getTextureManager().release(id);
        }
        texture = null;
        image = null;
        buffer = null;
        hasFrame = false;
    }
}
