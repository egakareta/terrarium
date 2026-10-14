#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

use std::sync::Arc;

use egui::{
    Color32, FontId, Galley, Pos2, Rect, Response, Sense, Stroke, TextureHandle, TextureOptions,
    Ui, Vec2, Widget, epaint::ColorImage,
};

const MAX_MASK_SIDE: usize = 4096;

/// A label whose glyphs are surrounded by a single rasterized outline.
///
/// Text is laid out by egui and painted normally. The outline is generated from
/// the glyph coverage in egui's font atlas and cached in the widget's egui
/// context state.
pub struct OutlinedLabel {
    text: String,
    font_id: Option<FontId>,
    text_color: Option<Color32>,
    outline: Stroke,
}

impl OutlinedLabel {
    /// Creates an outlined label with egui's body text style and text color.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_id: None,
            text_color: None,
            outline: Stroke::NONE,
        }
    }

    /// Sets the font used to lay out the text.
    #[must_use]
    pub fn font(mut self, font_id: FontId) -> Self {
        self.font_id = Some(font_id);
        self
    }

    /// Sets the fill color of the text.
    #[must_use]
    pub fn text_color(mut self, color: Color32) -> Self {
        self.text_color = Some(color);
        self
    }

    /// Sets the color and outward width of the outline, in egui points.
    #[must_use]
    pub fn outline(mut self, outline: Stroke) -> Self {
        self.outline = outline;
        self
    }
}

impl Widget for OutlinedLabel {
    fn ui(self, ui: &mut Ui) -> Response {
        let font_id = self
            .font_id
            .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
        let text_color = self.text_color.unwrap_or_else(|| ui.visuals().text_color());
        let painter = ui.painter().clone();
        let galley = painter.layout_no_wrap(self.text.clone(), font_id, text_color);

        let pixels_per_point = galley.pixels_per_point;
        let outline_width = if self.outline.width.is_finite() {
            self.outline.width.max(0.0)
        } else {
            0.0
        };
        let radius_pixels = outline_width * pixels_per_point;
        let can_outline = radius_pixels.is_finite()
            && radius_pixels > 0.0
            && radius_pixels < MAX_MASK_SIDE as f32
            && self.outline.color != Color32::TRANSPARENT
            && valid_bounds(galley.mesh_bounds);
        let padding_pixels = if can_outline {
            radius_pixels.ceil() as usize + 1
        } else {
            0
        };
        let padding_points = padding_pixels as f32 / pixels_per_point;
        let content_bounds = if can_outline {
            galley.rect.union(galley.mesh_bounds.expand(padding_points))
        } else {
            galley.rect
        };

        let (rect, response) = ui.allocate_exact_size(content_bounds.size(), Sense::hover());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), &self.text)
        });
        let galley_pos = rect.min - content_bounds.min.to_vec2();

        if can_outline && ui.is_rect_visible(rect) {
            let cache_id = response.id.with("egui_text_outline");
            let radius_key = radius_pixels.to_bits();
            let (is_cached, previous_texture) = ui.ctx().data_mut(|data| {
                let state = data.get_temp_mut_or_default::<OutlineState>(cache_id);
                let matches = state.galley.as_ref().is_some_and(|cached| {
                    Arc::ptr_eq(cached, &galley) && state.radius_pixels == radius_key
                });
                (matches, state.texture.clone())
            });

            let (texture, image_offset, image_size) = if is_cached {
                ui.ctx().data_mut(|data| {
                    let state = data.get_temp_mut_or_default::<OutlineState>(cache_id);
                    (state.texture.clone(), state.image_offset, state.image_size)
                })
            } else {
                let font_atlas = painter.fonts(|fonts| fonts.image());
                let max_texture_side = ui
                    .ctx()
                    .input(|input| input.max_texture_side)
                    .min(MAX_MASK_SIDE);
                let outline = make_outline_image(
                    &galley,
                    &font_atlas,
                    radius_pixels,
                    padding_pixels,
                    max_texture_side,
                );
                let (texture, image_offset, image_size) = match outline {
                    Some(outline) => {
                        let texture = if let Some(mut texture) = previous_texture {
                            texture.set(outline.image, TextureOptions::LINEAR);
                            texture
                        } else {
                            ui.ctx().load_texture(
                                "egui-text-outline",
                                outline.image,
                                TextureOptions::LINEAR,
                            )
                        };
                        (Some(texture), outline.offset, outline.size)
                    }
                    None => (None, Vec2::ZERO, Vec2::ZERO),
                };
                ui.ctx().data_mut(|data| {
                    let state = data.get_temp_mut_or_default::<OutlineState>(cache_id);
                    state.galley = Some(Arc::clone(&galley));
                    state.radius_pixels = radius_key;
                    state.texture = texture.clone();
                    state.image_offset = image_offset;
                    state.image_size = image_size;
                });
                (texture, image_offset, image_size)
            };

            if let Some(texture) = texture {
                let image_rect = Rect::from_min_size(galley_pos + image_offset, image_size);
                painter.image(
                    texture.id(),
                    image_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    self.outline.color,
                );
            }
        }

        painter.galley(galley_pos, galley, text_color);
        response
    }
}

#[derive(Clone, Default)]
struct OutlineState {
    galley: Option<Arc<Galley>>,
    radius_pixels: u32,
    texture: Option<TextureHandle>,
    image_offset: Vec2,
    image_size: Vec2,
}

struct OutlineImage {
    image: ColorImage,
    offset: Vec2,
    size: Vec2,
}

fn valid_bounds(bounds: Rect) -> bool {
    bounds.width().is_finite()
        && bounds.height().is_finite()
        && bounds.width() > 0.0
        && bounds.height() > 0.0
}

fn make_outline_image(
    galley: &Galley,
    font_atlas: &ColorImage,
    radius_pixels: f32,
    padding_pixels: usize,
    max_texture_side: usize,
) -> Option<OutlineImage> {
    let bounds = galley.mesh_bounds;
    let scale = galley.pixels_per_point;
    let base_width = (bounds.width() * scale).ceil() as usize;
    let base_height = (bounds.height() * scale).ceil() as usize;
    let width = base_width.checked_add(padding_pixels.checked_mul(2)?)?;
    let height = base_height.checked_add(padding_pixels.checked_mul(2)?)?;
    if width == 0 || height == 0 || width > max_texture_side || height > max_texture_side {
        return None;
    }

    let mut glyph_coverage = vec![0_u8; base_width.checked_mul(base_height)?];
    for placed_row in &galley.rows {
        let visuals = &placed_row.row.visuals;
        let mesh = &visuals.mesh;
        let glyph_vertices = &visuals.glyph_vertex_range;
        for triangle in mesh.indices.chunks_exact(3) {
            let indices = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if !indices.iter().all(|index| glyph_vertices.contains(index)) {
                continue;
            }
            let vertices = indices.map(|index| &mesh.vertices[index]);
            let positions = vertices.map(|vertex| {
                let pos = vertex.pos + placed_row.pos.to_vec2();
                Pos2::new(
                    (pos.x - bounds.min.x) * scale,
                    (pos.y - bounds.min.y) * scale,
                )
            });
            rasterize_triangle(
                &mut glyph_coverage,
                base_width,
                base_height,
                positions,
                vertices.map(|vertex| vertex.uv),
                font_atlas,
            );
        }
    }

    let coverage = dilate(
        &glyph_coverage,
        base_width,
        base_height,
        width,
        height,
        padding_pixels,
        radius_pixels,
    );
    let pixels = coverage
        .into_iter()
        .map(Color32::from_white_alpha)
        .collect();
    Some(OutlineImage {
        image: ColorImage::new([width, height], pixels),
        offset: bounds.min.to_vec2() - Vec2::splat(padding_pixels as f32 / scale),
        size: Vec2::new(width as f32 / scale, height as f32 / scale),
    })
}

fn rasterize_triangle(
    coverage: &mut [u8],
    width: usize,
    height: usize,
    positions: [Pos2; 3],
    uvs: [Pos2; 3],
    atlas: &ColorImage,
) {
    let denominator = (positions[1].y - positions[2].y) * (positions[0].x - positions[2].x)
        + (positions[2].x - positions[1].x) * (positions[0].y - positions[2].y);
    if denominator.abs() < f32::EPSILON {
        return;
    }

    let min_x = positions
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let min_y = positions
        .iter()
        .map(|point| point.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = positions
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(width as f32) as usize;
    let max_y = positions
        .iter()
        .map(|point| point.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(height as f32) as usize;

    for y in min_y..max_y {
        for x in min_x..max_x {
            let point = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
            let a = ((positions[1].y - positions[2].y) * (point.x - positions[2].x)
                + (positions[2].x - positions[1].x) * (point.y - positions[2].y))
                / denominator;
            let b = ((positions[2].y - positions[0].y) * (point.x - positions[2].x)
                + (positions[0].x - positions[2].x) * (point.y - positions[2].y))
                / denominator;
            let c = 1.0 - a - b;
            if a < 0.0 || b < 0.0 || c < 0.0 {
                continue;
            }

            let uv = Pos2::new(
                a * uvs[0].x + b * uvs[1].x + c * uvs[2].x,
                a * uvs[0].y + b * uvs[1].y + c * uvs[2].y,
            );
            let alpha = sample_alpha(atlas, uv);
            let index = y * width + x;
            coverage[index] = coverage[index].max(alpha);
        }
    }
}

fn sample_alpha(image: &ColorImage, uv: Pos2) -> u8 {
    let x = (uv.x - 0.5).clamp(0.0, image.width().saturating_sub(1) as f32);
    let y = (uv.y - 0.5).clamp(0.0, image.height().saturating_sub(1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(image.width() - 1);
    let y1 = (y0 + 1).min(image.height() - 1);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let alpha = |x, y| image[(x, y)].a() as f32;
    let top = alpha(x0, y0) + (alpha(x1, y0) - alpha(x0, y0)) * tx;
    let bottom = alpha(x0, y1) + (alpha(x1, y1) - alpha(x0, y1)) * tx;
    (top + (bottom - top) * ty).round() as u8
}

fn dilate(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    output_width: usize,
    output_height: usize,
    padding: usize,
    radius: f32,
) -> Vec<u8> {
    let mut output = vec![0; output_width * output_height];
    let extent = (radius + 0.5).ceil() as isize;
    for source_y in 0..source_height {
        for source_x in 0..source_width {
            let alpha = source[source_y * source_width + source_x];
            if alpha == 0 {
                continue;
            }
            for dy in -extent..=extent {
                for dx in -extent..=extent {
                    let distance = ((dx * dx + dy * dy) as f32).sqrt();
                    let edge_coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
                    if edge_coverage == 0.0 {
                        continue;
                    }
                    let output_x = source_x as isize + padding as isize + dx;
                    let output_y = source_y as isize + padding as isize + dy;
                    if output_x < 0
                        || output_y < 0
                        || output_x >= output_width as isize
                        || output_y >= output_height as isize
                    {
                        continue;
                    }
                    let index = output_y as usize * output_width + output_x as usize;
                    let expanded = (alpha as f32 * edge_coverage).round() as u8;
                    output[index] = output[index].max(expanded);
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::dilate;

    #[test]
    fn dilation_expands_a_mask_and_preserves_its_center() {
        let source = [0, 0, 0, 0, 255, 0, 0, 0, 0];
        let output = dilate(&source, 3, 3, 7, 7, 2, 1.0);

        assert_eq!(output[3 * 7 + 3], 255);
        assert!(output[3 * 7 + 4] > 0);
        assert_eq!(output[0], 0);
    }
}
