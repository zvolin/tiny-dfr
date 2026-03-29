//! Software rasterizer using tiny-skia

use egui::{ClippedPrimitive, TexturesDelta, TextureId, ImageData, Rect, Mesh, Color32};
use egui::epaint::Primitive;
use std::collections::HashMap;
use tiny_skia::{Pixmap, Color};

/// Software rasterizer for egui primitives
pub struct TinySkiaPainter {
    pixmap: Pixmap,
    textures: HashMap<TextureId, Pixmap>,
}

impl TinySkiaPainter {
    /// Create a new painter with the given dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            pixmap: Pixmap::new(width, height).unwrap(),
            textures: HashMap::new(),
        }
    }

    /// Paint primitives with texture updates
    pub fn paint(&mut self, primitives: &[ClippedPrimitive], textures_delta: &TexturesDelta) {
        // Update textures
        for (id, delta) in &textures_delta.set {
            let is_partial = delta.pos.is_some();
            let (offset_x, offset_y) = delta.pos.map(|p| (p[0], p[1])).unwrap_or((0, 0));

            if is_partial {
                // Partial update - write into existing texture
                if let Some(existing) = self.textures.get_mut(id) {
                    match &delta.image {
                        ImageData::Color(img) => {
                            let tex_width = existing.width() as usize;
                            for (i, pixel) in img.pixels.iter().enumerate() {
                                let src_x = i % img.width();
                                let src_y = i / img.width();
                                let dst_x = offset_x + src_x;
                                let dst_y = offset_y + src_y;
                                let idx = (dst_y * tex_width + dst_x) * 4;
                                if idx + 3 < existing.data_mut().len() {
                                    existing.data_mut()[idx] = pixel.r();
                                    existing.data_mut()[idx + 1] = pixel.g();
                                    existing.data_mut()[idx + 2] = pixel.b();
                                    existing.data_mut()[idx + 3] = pixel.a();
                                }
                            }
                        }
                        ImageData::Font(font) => {
                            let tex_width = existing.width() as usize;
                            for (i, coverage) in font.pixels.iter().enumerate() {
                                let src_x = i % font.width();
                                let src_y = i / font.width();
                                let dst_x = offset_x + src_x;
                                let dst_y = offset_y + src_y;
                                let idx = (dst_y * tex_width + dst_x) * 4;
                                let alpha = (coverage * 255.0) as u8;
                                if idx + 3 < existing.data_mut().len() {
                                    existing.data_mut()[idx] = 255;
                                    existing.data_mut()[idx + 1] = 255;
                                    existing.data_mut()[idx + 2] = 255;
                                    existing.data_mut()[idx + 3] = alpha;
                                }
                            }
                        }
                    }
                }
            } else {
                // Full texture replacement
                let image = match &delta.image {
                    ImageData::Color(img) => {
                        let mut pixmap = Pixmap::new(img.width() as u32, img.height() as u32).unwrap();
                        for (i, pixel) in img.pixels.iter().enumerate() {
                            let idx = i * 4;
                            pixmap.data_mut()[idx] = pixel.r();
                            pixmap.data_mut()[idx + 1] = pixel.g();
                            pixmap.data_mut()[idx + 2] = pixel.b();
                            pixmap.data_mut()[idx + 3] = pixel.a();
                        }
                        pixmap
                    }
                    ImageData::Font(font) => {
                        let mut pixmap = Pixmap::new(font.width() as u32, font.height() as u32).unwrap();
                        for (i, coverage) in font.pixels.iter().enumerate() {
                            let idx = i * 4;
                            let alpha = (coverage * 255.0) as u8;
                            // Font textures are white with alpha from coverage
                            pixmap.data_mut()[idx] = 255;
                            pixmap.data_mut()[idx + 1] = 255;
                            pixmap.data_mut()[idx + 2] = 255;
                            pixmap.data_mut()[idx + 3] = alpha;
                        }
                        pixmap
                    }
                };
                self.textures.insert(*id, image);
            }
        }

        // Clear to black
        self.pixmap.fill(Color::BLACK);

        // Paint primitives
        for primitive in primitives {
            if let Primitive::Mesh(mesh) = &primitive.primitive {
                self.paint_mesh(mesh, &primitive.clip_rect);
            }
        }

        // Free old textures
        for id in &textures_delta.free {
            self.textures.remove(id);
        }
    }

    fn paint_mesh(&mut self, mesh: &Mesh, clip: &Rect) {
        let texture = self.textures.get(&mesh.texture_id);
        let width = self.pixmap.width() as i32;
        let height = self.pixmap.height() as i32;

        // Clip bounds
        let clip_min_x = clip.min.x.max(0.0) as i32;
        let clip_min_y = clip.min.y.max(0.0) as i32;
        let clip_max_x = (clip.max.x as i32).min(width);
        let clip_max_y = (clip.max.y as i32).min(height);

        for indices in mesh.indices.chunks(3) {
            let v0 = &mesh.vertices[indices[0] as usize];
            let v1 = &mesh.vertices[indices[1] as usize];
            let v2 = &mesh.vertices[indices[2] as usize];

            // Bounding box of triangle
            let min_x = v0.pos.x.min(v1.pos.x).min(v2.pos.x).floor() as i32;
            let min_y = v0.pos.y.min(v1.pos.y).min(v2.pos.y).floor() as i32;
            let max_x = v0.pos.x.max(v1.pos.x).max(v2.pos.x).ceil() as i32;
            let max_y = v0.pos.y.max(v1.pos.y).max(v2.pos.y).ceil() as i32;

            // Clamp to clip rect and screen
            let min_x = min_x.max(clip_min_x).max(0);
            let min_y = min_y.max(clip_min_y).max(0);
            let max_x = max_x.min(clip_max_x).min(width);
            let max_y = max_y.min(clip_max_y).min(height);

            // Rasterize triangle
            for py in min_y..max_y {
                for px in min_x..max_x {
                    let p = (px as f32 + 0.5, py as f32 + 0.5);

                    // Compute barycentric coordinates
                    if let Some((u, v, w)) = barycentric(
                        (v0.pos.x, v0.pos.y),
                        (v1.pos.x, v1.pos.y),
                        (v2.pos.x, v2.pos.y),
                        p,
                    ) {
                        // Interpolate UV coordinates
                        let tex_u = v0.uv.x * u + v1.uv.x * v + v2.uv.x * w;
                        let tex_v = v0.uv.y * u + v1.uv.y * v + v2.uv.y * w;

                        // Interpolate vertex color
                        let r = (v0.color.r() as f32 * u + v1.color.r() as f32 * v + v2.color.r() as f32 * w) as u8;
                        let g = (v0.color.g() as f32 * u + v1.color.g() as f32 * v + v2.color.g() as f32 * w) as u8;
                        let b = (v0.color.b() as f32 * u + v1.color.b() as f32 * v + v2.color.b() as f32 * w) as u8;
                        let a = (v0.color.a() as f32 * u + v1.color.a() as f32 * v + v2.color.a() as f32 * w) as u8;

                        // Sample texture if present
                        let color = if let Some(tex) = texture {
                            sample_texture(tex, tex_u, tex_v, Color32::from_rgba_unmultiplied(r, g, b, a))
                        } else {
                            Color32::from_rgba_unmultiplied(r, g, b, a)
                        };

                        // Blend with existing pixel
                        let idx = ((py as u32 * self.pixmap.width() + px as u32) * 4) as usize;
                        let data = self.pixmap.data_mut();
                        if idx + 3 < data.len() {
                            blend_pixel(&mut data[idx..idx + 4], color);
                        }
                    }
                }
            }
        }
    }

    /// Get the raw pixel data
    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }
}

/// Compute barycentric coordinates of point p in triangle (a, b, c)
/// Returns None if point is outside triangle
fn barycentric(a: (f32, f32), b: (f32, f32), c: (f32, f32), p: (f32, f32)) -> Option<(f32, f32, f32)> {
    let v0 = (c.0 - a.0, c.1 - a.1);
    let v1 = (b.0 - a.0, b.1 - a.1);
    let v2 = (p.0 - a.0, p.1 - a.1);

    let dot00 = v0.0 * v0.0 + v0.1 * v0.1;
    let dot01 = v0.0 * v1.0 + v0.1 * v1.1;
    let dot02 = v0.0 * v2.0 + v0.1 * v2.1;
    let dot11 = v1.0 * v1.0 + v1.1 * v1.1;
    let dot12 = v1.0 * v2.0 + v1.1 * v2.1;

    let inv_denom = 1.0 / (dot00 * dot11 - dot01 * dot01);
    let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
    let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
    let w = 1.0 - u - v;

    if u >= 0.0 && v >= 0.0 && w >= 0.0 {
        Some((w, v, u))
    } else {
        None
    }
}

/// Sample texture at UV coordinates, multiply by vertex color
fn sample_texture(tex: &Pixmap, u: f32, v: f32, vertex_color: Color32) -> Color32 {
    let tw = tex.width() as f32;
    let th = tex.height() as f32;

    let tx = ((u * tw) as i32).clamp(0, tex.width() as i32 - 1) as u32;
    let ty = ((v * th) as i32).clamp(0, tex.height() as i32 - 1) as u32;

    let idx = ((ty * tex.width() + tx) * 4) as usize;
    let data = tex.data();

    if idx + 3 < data.len() {
        let tex_r = data[idx];
        let tex_g = data[idx + 1];
        let tex_b = data[idx + 2];
        let tex_a = data[idx + 3];

        // Multiply texture color with vertex color
        let r = ((tex_r as u32 * vertex_color.r() as u32) / 255) as u8;
        let g = ((tex_g as u32 * vertex_color.g() as u32) / 255) as u8;
        let b = ((tex_b as u32 * vertex_color.b() as u32) / 255) as u8;
        let a = ((tex_a as u32 * vertex_color.a() as u32) / 255) as u8;

        Color32::from_rgba_unmultiplied(r, g, b, a)
    } else {
        vertex_color
    }
}

/// Alpha blend color onto destination pixel
fn blend_pixel(dst: &mut [u8], src: Color32) {
    let src_a = src.a() as f32 / 255.0;
    if src_a == 0.0 {
        return;
    }

    let dst_a = dst[3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);

    if out_a > 0.0 {
        dst[0] = ((src.r() as f32 * src_a + dst[0] as f32 * dst_a * (1.0 - src_a)) / out_a) as u8;
        dst[1] = ((src.g() as f32 * src_a + dst[1] as f32 * dst_a * (1.0 - src_a)) / out_a) as u8;
        dst[2] = ((src.b() as f32 * src_a + dst[2] as f32 * dst_a * (1.0 - src_a)) / out_a) as u8;
        dst[3] = (out_a * 255.0) as u8;
    }
}
