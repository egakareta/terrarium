use std::sync::OnceLock;

use thiserror::Error;

use crate::Color3;

/// Depth format used by the built-in renderer.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Number of directional material slots available to a mesh.
pub const MATERIAL_SLOT_COUNT: usize = 6;

/// A handle to a texture stored in a [`crate::Workspace`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureHandle(
    /// Index into the CPU texture storage owned by a [`crate::Workspace`].
    pub usize,
);

/// The material slot selected by a mesh vertex.
///
/// Directional slots describe the orientation of a face in the mesh's local
/// space, so they apply to any mesh.
///
/// [`Top`](Self::Top) faces toward `+Y`,
/// [`Bottom`](Self::Bottom) toward `-Y`,
/// [`Front`](Self::Front) toward `+Z`,
/// [`Back`](Self::Back) toward `-Z`,
/// [`Right`](Self::Right) toward `+X`,
/// and [`Left`](Self::Left) toward `-X`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterialSlot {
    /// The top-facing material slot.
    Top = 0,
    /// The bottom-facing material slot.
    Bottom = 1,
    /// The front-facing material slot toward `+Z`.
    Front = 2,
    /// The back-facing material slot toward `-Z`.
    Back = 3,
    /// The left-facing material slot toward `-X`.
    Left = 4,
    /// The right-facing material slot toward `+X`.
    Right = 5,
}

impl MaterialSlot {
    /// All directional slots in GPU slot order.
    pub const ALL_DIRECTIONS: [Self; MATERIAL_SLOT_COUNT] = [
        Self::Top,
        Self::Bottom,
        Self::Front,
        Self::Back,
        Self::Left,
        Self::Right,
    ];

    /// Returns the zero-based GPU slot index.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Selects the directional slot whose axis best matches a face normal.
    ///
    /// The dominant axis of `normal` wins: `|y|` beats `|x|` beats `|z|` on
    /// ties, so an up-facing slope maps to [`Top`](Self::Top) rather than a
    /// side. A zero normal maps to [`Top`](Self::Top).
    pub fn from_normal(normal: [f32; 3]) -> Self {
        let [x, y, z] = normal;
        let ax = x.abs();
        let ay = y.abs();
        let az = z.abs();
        if ax == 0.0 && ay == 0.0 && az == 0.0 {
            return Self::Top;
        }
        if ay >= ax && ay >= az {
            if y >= 0.0 { Self::Top } else { Self::Bottom }
        } else if ax >= az {
            if x >= 0.0 { Self::Right } else { Self::Left }
        } else if z >= 0.0 {
            Self::Front
        } else {
            Self::Back
        }
    }
}

/// Texture maps used by a PBR material.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureSet {
    /// Optional sRGB base-color texture.
    pub base_color: Option<TextureHandle>,
    /// Optional linear tangent-space normal texture.
    pub normal: Option<TextureHandle>,
    /// Optional linear glTF metallic-roughness texture.
    pub metallic_roughness: Option<TextureHandle>,
    /// Optional sRGB emissive texture.
    pub emissive: Option<TextureHandle>,
}

/// PBR factors and optional texture maps used by a [`crate::Part`].
///
/// The metallic-roughness map follows the glTF convention: metallic is read
/// from the blue channel and roughness from the green channel. Scalar factors
/// are resolved per face, so each directional [`MaterialSlot`] renders its own
/// base color, metallic, roughness, and emissive values; the part tint
/// ([`crate::BasePart::color`]) still multiplies every face.
///
/// Texture sampling for each slot uses that slot's [`Material::filter`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// RGBA multiplier for the base color. Values are not clamped on assignment.
    pub base_color: [f32; 4],
    /// Metallic factor, clamped to `0.0..=1.0` during rendering.
    pub metallic: f32,
    /// Roughness factor, clamped to `0.04..=1.0` during rendering.
    pub roughness: f32,
    /// RGB emissive contribution added after direct and ambient lighting.
    pub emissive: [f32; 3],
    /// Optional texture maps multiplied by the scalar factors.
    pub textures: TextureSet,
    /// Texture sampling filter used for this material's texture maps.
    pub filter: TextureFilter,
}

impl Default for Material {
    fn default() -> Self {
        DEFAULT_MATERIAL
    }
}

pub(crate) const DEFAULT_MATERIAL: Material = Material {
    base_color: [1.0, 1.0, 1.0, 1.0],
    metallic: 0.0,
    roughness: 0.5,
    emissive: [0.0, 0.0, 0.0],
    textures: TextureSet {
        base_color: None,
        normal: None,
        metallic_roughness: None,
        emissive: None,
    },
    filter: TextureFilter::Trilinear,
};

impl Material {
    /// Creates a material using a texture as its base-color map.
    pub fn textured(texture: TextureHandle) -> Self {
        Self::default().with_base_color_texture(texture)
    }

    /// Returns a copy of this material with a base-color map assigned.
    pub fn with_base_color_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.base_color = Some(texture);
        self
    }

    /// Returns a copy of this material with a normal map assigned.
    pub fn with_normal_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.normal = Some(texture);
        self
    }

    /// Returns a copy of this material with a metallic-roughness map assigned.
    pub fn with_metallic_roughness_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.metallic_roughness = Some(texture);
        self
    }

    /// Returns a copy of this material with an emissive map assigned.
    pub fn with_emissive_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.emissive = Some(texture);
        self
    }

    /// Returns a copy of this material with a texture sampling filter assigned.
    ///
    /// Each directional slot samples with its own filter, so set the same
    /// filter on every slot material when they should match. Unset slots fall
    /// back to the default material's filter.
    pub fn with_filter(mut self, filter: TextureFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Creates a material whose base color comes from an RGB tint.
    pub fn from_color(color: Color3) -> Self {
        Self {
            base_color: color.rgba(),
            ..Self::default()
        }
    }
}

/// Materials assigned to the directional slots of a mesh.
///
/// The first element in this list is [`MaterialSlot::Top`], followed by the
/// remaining slots in [`MaterialSlot::ALL_DIRECTIONS`] order. Entries are
/// `Some` only for slots assigned through [`MeshMaterialSlots::set`]; skipped
/// slots stay `None` and fall back to [`Material::default()`] at render time.
///
/// Slots are directional (see [`MaterialSlot`]), so the same slot names work
/// for every shape: setting [`MaterialSlot::Top`] affects the top-facing
/// triangles of a [`crate::PartShape::Block`], [`crate::PartShape::Cylinder`],
/// [`crate::PartShape::Wedge`], or any custom [`crate::Mesh`] whose vertices
/// are tagged by orientation. Unset slots fall back to the default material at
/// render time, including its scalar PBR factors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshMaterialSlots {
    /// Materials stored by directional slot order.
    ///
    /// `None` entries were never assigned and fall back to
    /// [`Material::default()`].
    pub slots: Vec<Option<Material>>,
}

impl MeshMaterialSlots {
    /// Assigns a directional material slot, growing the slot list as needed.
    ///
    /// Skipped slots stay `None` and fall back to [`Material::default()`].
    pub fn set(&mut self, slot: MaterialSlot, material: Material) {
        let index = slot.index();
        if self.slots.len() <= index {
            self.slots.resize(index + 1, None);
        }
        self.slots[index] = Some(material);
    }

    /// Returns the material assigned to a directional slot, if any.
    pub fn get(&self, slot: MaterialSlot) -> Option<&Material> {
        self.slots.get(slot.index())?.as_ref()
    }

    /// Returns the mutable material assigned to a directional slot, if any.
    pub fn get_mut(&mut self, slot: MaterialSlot) -> Option<&mut Material> {
        self.slots.get_mut(slot.index())?.as_mut()
    }
}

/// The color space used when sampling an RGBA8 texture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextureColorSpace {
    /// Color data such as a base-color map. The GPU converts it to linear space.
    Srgb,
    /// Data maps such as normals and metallic-roughness.
    Linear,
}

/// Texture sampling filter used by a [`Material`].
///
/// All variants repeat the texture on wrap. Mipmaps are always generated; the
/// `Nearest`/`Bilinear` variants only change whether the closest mip level is
/// picked (`Nearest`) or two levels are blended (`Linear`).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextureFilter {
    /// Point sampling for magnification, minification, and mips. Best for
    /// pixel art where texels should stay crisp.
    Nearest,
    /// Bilinear magnification/minification with the nearest mip level. The
    /// classic "bilinear" look.
    Bilinear,
    /// Bilinear magnification/minification blended across mip levels. This is
    /// the default and matches the previous renderer behavior.
    #[default]
    Trilinear,
    /// Trilinear sampling with `4x` anisotropic filtering for grazing angles.
    Anisotropic4x,
    /// Trilinear sampling with `8x` anisotropic filtering for grazing angles.
    Anisotropic8x,
    /// Trilinear sampling with `16x` anisotropic filtering for grazing angles.
    Anisotropic16x,
}

impl TextureFilter {
    /// All filter modes in a stable order, useful for pre-creating samplers.
    pub const ALL: [Self; 6] = [
        Self::Nearest,
        Self::Bilinear,
        Self::Trilinear,
        Self::Anisotropic4x,
        Self::Anisotropic8x,
        Self::Anisotropic16x,
    ];
}

/// CPU-side image data independent of GPU texture resources.
#[derive(Clone, Debug)]
pub struct Image {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Packed row-major RGBA8 pixels, four bytes per pixel.
    pub pixels: Vec<u8>,
}

impl Image {
    /// Decodes any image format supported by the configured image codecs.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        let image = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = image.dimensions();
        Self::from_rgba8(width, height, image.into_raw())
    }

    /// Creates an image from RGBA8 pixels.
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        let image = Self {
            width,
            height,
            pixels,
        };
        image.validate()?;
        Ok(image)
    }

    /// Returns the RGBA8 pixel at (`x`, `y`), with the origin at the top-left.
    ///
    /// # Panics
    ///
    /// Panics if (`x`, `y`) is outside the image dimensions.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let start = pixel_index(self.width, self.height, x, y);
        self.pixels[start..start + 4]
            .try_into()
            .expect("pixel index is within validated pixel data")
    }

    /// Sets the RGBA8 pixel at (`x`, `y`) and returns this image for chaining.
    ///
    /// # Panics
    ///
    /// Panics if (`x`, `y`) is outside the image dimensions.
    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: [u8; 4]) -> &mut Self {
        let start = pixel_index(self.width, self.height, x, y);
        self.pixels[start..start + 4].copy_from_slice(&pixel);
        self
    }

    /// Returns the packed row-major RGBA8 pixels, four bytes per pixel.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns the packed row-major RGBA8 pixels for manual editing.
    ///
    /// The buffer must stay tightly packed RGBA8 (`width * height * 4` bytes);
    /// changing its length makes the image invalid.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Mirrors the image left-to-right and returns it for chaining.
    pub fn flip_horizontal(mut self) -> Self {
        flip_horizontal_in_place(self.width, self.height, &mut self.pixels);
        self
    }

    /// Mirrors the image top-to-bottom and returns it for chaining.
    pub fn flip_vertical(mut self) -> Self {
        flip_vertical_in_place(self.width, self.height, &mut self.pixels);
        self
    }

    /// Rotates the image 90 degrees clockwise and returns it for chaining.
    ///
    /// Width and height are swapped.
    pub fn rotate90(mut self) -> Self {
        rotate90_cw_in_place(&mut self.width, &mut self.height, &mut self.pixels);
        self
    }

    /// Rotates the image 180 degrees and returns it for chaining.
    pub fn rotate180(mut self) -> Self {
        rotate180_in_place(&mut self.pixels);
        self
    }

    /// Rotates the image 270 degrees clockwise (90 degrees
    /// counter-clockwise) and returns it for chaining.
    ///
    /// Width and height are swapped.
    pub fn rotate270(mut self) -> Self {
        rotate270_cw_in_place(&mut self.width, &mut self.height, &mut self.pixels);
        self
    }

    /// Multiplies every channel by the matching factor and returns the image
    /// for chaining.
    ///
    /// Each result is rounded and clamped to `0..=255`. Pass `1.0` for a
    /// channel that should stay unchanged, so `[0.8, 1.0, 0.8, 1.0]` darkens
    /// red and blue while keeping green and alpha intact.
    pub fn tint(mut self, multiplier: [f32; 4]) -> Self {
        tint_in_place(&mut self.pixels, multiplier);
        self
    }

    /// Converts the image to grayscale and returns it for chaining.
    ///
    /// Each pixel's RGB channels are replaced by their byte-space luma while
    /// alpha is preserved.
    pub fn grayscale(mut self) -> Self {
        grayscale_in_place(&mut self.pixels);
        self
    }

    /// Inverts the RGB channels (`255 - value`) and returns the image for
    /// chaining. Alpha is preserved.
    pub fn invert_rgb(mut self) -> Self {
        invert_rgb_in_place(&mut self.pixels);
        self
    }

    fn validate(&self) -> Result<(), TextureError> {
        if self.width == 0 || self.height == 0 {
            return Err(TextureError::ZeroDimensions);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(TextureError::DimensionsTooLarge)?;
        if self.pixels.len() != expected {
            return Err(TextureError::InvalidData {
                actual: self.pixels.len(),
                expected,
            });
        }
        Ok(())
    }
}

/// CPU-side RGBA8 texture data ready to be uploaded to a [`Renderer`].
///
/// The geometric transforms ([`flip_horizontal`](Self::flip_horizontal),
/// [`flip_vertical`](Self::flip_vertical), [`rotate90`](Self::rotate90),
/// [`rotate180`](Self::rotate180), [`rotate270`](Self::rotate270)) move texels
/// unchanged. That is correct for color and data maps, but tangent-space
/// normal maps encode direction: after mirroring or rotating a normal map,
/// adjust its red/green channels to match the new orientation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Texture {
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
    /// Packed row-major RGBA8 pixels, four bytes per pixel.
    pub pixels: Vec<u8>,
    /// Whether the bytes should be sampled as color or data.
    pub color_space: TextureColorSpace,
}

/// Errors produced while decoding or validating image and texture data.
#[derive(Debug, Error)]
pub enum TextureError {
    /// The image decoder could not read the supplied bytes.
    #[error("could not decode image data: {0}")]
    ImageDecode(#[from] image::ImageError),
    /// Width or height was zero.
    #[error("texture dimensions must be greater than zero")]
    ZeroDimensions,
    /// The dimensions overflowed while calculating the expected byte count.
    #[error("texture dimensions are too large")]
    DimensionsTooLarge,
    /// The pixel buffer length did not equal `width * height * 4`.
    #[error("RGBA8 texture data has {actual} bytes, expected {expected}")]
    InvalidData {
        /// Number of bytes supplied by the caller.
        actual: usize,
        /// Number of bytes required by the dimensions.
        expected: usize,
    },
    /// A workspace texture handle did not identify a stored texture.
    #[error("workspace texture handle {index} is not valid")]
    InvalidHandle {
        /// The invalid workspace-local texture index.
        index: usize,
    },
}

impl Texture {
    /// Creates an sRGB RGBA8 texture, suitable for a base-color map.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        Self::with_color_space(width, height, pixels, TextureColorSpace::Srgb)
    }

    /// Creates an RGBA8 texture with an explicit color space.
    pub fn with_color_space(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        color_space: TextureColorSpace,
    ) -> Result<Self, TextureError> {
        let texture = Self {
            width,
            height,
            pixels,
            color_space,
        };
        texture.validate()?;
        Ok(texture)
    }

    /// Creates a texture from decoded image data.
    pub fn from_image(image: Image, color_space: TextureColorSpace) -> Result<Self, TextureError> {
        Self::with_color_space(image.width, image.height, image.pixels, color_space)
    }

    /// Creates a linear RGBA8 texture, suitable for normal or metallic-roughness data.
    pub fn linear(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        Self::with_color_space(width, height, pixels, TextureColorSpace::Linear)
    }

    /// Decodes an image from memory with an explicit texture color space.
    pub fn from_bytes(bytes: &[u8], color_space: TextureColorSpace) -> Result<Self, TextureError> {
        Self::from_image(Image::from_bytes(bytes)?, color_space)
    }

    /// Returns the RGBA8 pixel at (`x`, `y`), with the origin at the top-left.
    ///
    /// # Panics
    ///
    /// Panics if (`x`, `y`) is outside the texture dimensions.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let start = pixel_index(self.width, self.height, x, y);
        self.pixels[start..start + 4]
            .try_into()
            .expect("pixel index is within validated pixel data")
    }

    /// Sets the RGBA8 pixel at (`x`, `y`) and returns this texture for
    /// chaining.
    ///
    /// # Panics
    ///
    /// Panics if (`x`, `y`) is outside the texture dimensions.
    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: [u8; 4]) -> &mut Self {
        let start = pixel_index(self.width, self.height, x, y);
        self.pixels[start..start + 4].copy_from_slice(&pixel);
        self
    }

    /// Returns the packed row-major RGBA8 pixels, four bytes per pixel.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns the packed row-major RGBA8 pixels for manual editing.
    ///
    /// The buffer must stay tightly packed RGBA8 (`width * height * 4` bytes);
    /// changing its length makes the texture invalid.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// Mirrors the texture left-to-right and returns it for chaining.
    pub fn flip_horizontal(mut self) -> Self {
        flip_horizontal_in_place(self.width, self.height, &mut self.pixels);
        self
    }

    /// Mirrors the texture top-to-bottom and returns it for chaining.
    pub fn flip_vertical(mut self) -> Self {
        flip_vertical_in_place(self.width, self.height, &mut self.pixels);
        self
    }

    /// Rotates the texture 90 degrees clockwise and returns it for chaining.
    ///
    /// Width and height are swapped.
    pub fn rotate90(mut self) -> Self {
        rotate90_cw_in_place(&mut self.width, &mut self.height, &mut self.pixels);
        self
    }

    /// Rotates the texture 180 degrees and returns it for chaining.
    pub fn rotate180(mut self) -> Self {
        rotate180_in_place(&mut self.pixels);
        self
    }

    /// Rotates the texture 270 degrees clockwise (90 degrees
    /// counter-clockwise) and returns it for chaining.
    ///
    /// Width and height are swapped.
    pub fn rotate270(mut self) -> Self {
        rotate270_cw_in_place(&mut self.width, &mut self.height, &mut self.pixels);
        self
    }

    /// Multiplies every channel by the matching factor and returns the
    /// texture for chaining.
    ///
    /// Each result is rounded and clamped to `0..=255`. Pass `1.0` for a
    /// channel that should stay unchanged, so `[0.8, 1.0, 0.8, 1.0]` darkens
    /// red and blue while keeping green and alpha intact.
    pub fn tint(mut self, multiplier: [f32; 4]) -> Self {
        tint_in_place(&mut self.pixels, multiplier);
        self
    }

    /// Converts the texture to grayscale and returns it for chaining.
    ///
    /// Each pixel's RGB channels are replaced by their byte-space luma while
    /// alpha is preserved.
    pub fn grayscale(mut self) -> Self {
        grayscale_in_place(&mut self.pixels);
        self
    }

    /// Inverts the RGB channels (`255 - value`) and returns the texture for
    /// chaining. Alpha is preserved.
    pub fn invert_rgb(mut self) -> Self {
        invert_rgb_in_place(&mut self.pixels);
        self
    }

    /// Returns the number of mip levels in the automatically generated chain.
    pub fn mip_level_count(&self) -> u32 {
        let max_dimension = self.width.max(self.height);
        if max_dimension == 0 {
            0
        } else {
            max_dimension.ilog2() + 1
        }
    }

    /// Generates the complete mip chain, including the original image at level zero.
    ///
    /// RGB channels in sRGB textures are averaged in linear space before being
    /// encoded again. Linear textures, including normal and metallic-roughness
    /// maps, are averaged directly. The last row or column is included when a
    /// dimension is odd.
    pub fn mip_levels(&self) -> Result<Vec<Image>, TextureError> {
        self.validate()?;

        let mut levels = Vec::with_capacity(self.mip_level_count() as usize);
        let mut width = self.width;
        let mut height = self.height;
        let pixels = self.pixels.clone();
        levels.push(Image {
            width,
            height,
            pixels,
        });

        while width > 1 || height > 1 {
            let next_width = (width / 2).max(1);
            let next_height = (height / 2).max(1);
            let pixels = downsample_rgba8(
                width,
                height,
                next_width,
                next_height,
                &levels.last().expect("base mip was inserted").pixels,
                self.color_space,
            );
            width = next_width;
            height = next_height;
            levels.push(Image {
                width,
                height,
                pixels,
            });
        }

        Ok(levels)
    }

    /// Checks dimensions and verifies that the pixel buffer is tightly packed RGBA8.
    pub fn validate(&self) -> Result<(), TextureError> {
        if self.width == 0 || self.height == 0 {
            return Err(TextureError::ZeroDimensions);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(TextureError::DimensionsTooLarge)?;
        if self.pixels.len() != expected {
            return Err(TextureError::InvalidData {
                actual: self.pixels.len(),
                expected,
            });
        }
        Ok(())
    }
}

fn pixel_index(width: u32, height: u32, x: u32, y: u32) -> usize {
    assert!(
        x < width && y < height,
        "pixel ({x}, {y}) is outside a {width}x{height} image"
    );
    (y as usize * width as usize + x as usize) * 4
}

fn flip_horizontal_in_place(width: u32, height: u32, pixels: &mut [u8]) {
    debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
    let row_pixels = width as usize;
    for row in pixels.chunks_exact_mut(row_pixels * 4) {
        for x in 0..row_pixels / 2 {
            let left = x * 4;
            let right = (row_pixels - 1 - x) * 4;
            for channel in 0..4 {
                row.swap(left + channel, right + channel);
            }
        }
    }
}

fn flip_vertical_in_place(width: u32, height: u32, pixels: &mut [u8]) {
    debug_assert_eq!(pixels.len(), width as usize * height as usize * 4);
    let row_bytes = width as usize * 4;
    let rows = height as usize;
    for y in 0..rows / 2 {
        let (upper, lower) = pixels.split_at_mut((rows - 1 - y) * row_bytes);
        upper[y * row_bytes..(y + 1) * row_bytes].swap_with_slice(&mut lower[..row_bytes]);
    }
}

fn rotate180_in_place(pixels: &mut [u8]) {
    let texels = pixels.len() / 4;
    for index in 0..texels / 2 {
        let other = texels - 1 - index;
        for channel in 0..4 {
            pixels.swap(index * 4 + channel, other * 4 + channel);
        }
    }
}

/// 90 degrees clockwise: destination (`x`, `y`) samples source
/// (`y`, `source_height - 1 - x`) and the dimensions swap.
fn rotate90_cw_in_place(width: &mut u32, height: &mut u32, pixels: &mut Vec<u8>) {
    let (source_width, source_height) = (*width as usize, *height as usize);
    let mut rotated = vec![0u8; pixels.len()];
    for y in 0..source_width {
        for x in 0..source_height {
            let source = ((source_height - 1 - x) * source_width + y) * 4;
            let destination = (y * source_height + x) * 4;
            rotated[destination..destination + 4].copy_from_slice(&pixels[source..source + 4]);
        }
    }
    *pixels = rotated;
    std::mem::swap(width, height);
}

/// 270 degrees clockwise: destination (`x`, `y`) samples source
/// (`source_width - 1 - y`, `x`) and the dimensions swap.
fn rotate270_cw_in_place(width: &mut u32, height: &mut u32, pixels: &mut Vec<u8>) {
    let (source_width, source_height) = (*width as usize, *height as usize);
    let mut rotated = vec![0u8; pixels.len()];
    for y in 0..source_width {
        for x in 0..source_height {
            let source = (x * source_width + (source_width - 1 - y)) * 4;
            let destination = (y * source_height + x) * 4;
            rotated[destination..destination + 4].copy_from_slice(&pixels[source..source + 4]);
        }
    }
    *pixels = rotated;
    std::mem::swap(width, height);
}

fn tint_in_place(pixels: &mut [u8], multiplier: [f32; 4]) {
    for texel in pixels.chunks_exact_mut(4) {
        for (channel, factor) in texel.iter_mut().zip(multiplier) {
            *channel = (*channel as f32 * factor).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn grayscale_in_place(pixels: &mut [u8]) {
    for texel in pixels.chunks_exact_mut(4) {
        let luma = (0.2126 * f32::from(texel[0])
            + 0.7152 * f32::from(texel[1])
            + 0.0722 * f32::from(texel[2]))
        .round()
        .clamp(0.0, 255.0) as u8;
        texel[0] = luma;
        texel[1] = luma;
        texel[2] = luma;
    }
}

fn invert_rgb_in_place(pixels: &mut [u8]) {
    for texel in pixels.chunks_exact_mut(4) {
        texel[0] = 255 - texel[0];
        texel[1] = 255 - texel[1];
        texel[2] = 255 - texel[2];
    }
}

fn downsample_rgba8(
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
    source: &[u8],
    color_space: TextureColorSpace,
) -> Vec<u8> {
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        let source_y_start = (y as u64 * source_height as u64 / height as u64) as u32;
        let source_y_end = ((y as u64 + 1) * source_height as u64)
            .div_ceil(height as u64)
            .min(source_height as u64) as u32;
        for x in 0..width {
            let source_x_start = (x as u64 * source_width as u64 / width as u64) as u32;
            let source_x_end = ((x as u64 + 1) * source_width as u64)
                .div_ceil(width as u64)
                .min(source_width as u64) as u32;
            let mut sums = [0.0; 4];
            let mut sample_count = 0.0;
            for source_y in source_y_start..source_y_end {
                for source_x in source_x_start..source_x_end {
                    let source_index =
                        (source_y as usize * source_width as usize + source_x as usize) * 4;
                    for channel in 0..4 {
                        let value = source[source_index + channel];
                        sums[channel] += if color_space == TextureColorSpace::Srgb && channel < 3 {
                            srgb_to_linear(value)
                        } else {
                            value as f32 / 255.0
                        };
                    }
                    sample_count += 1.0;
                }
            }

            let destination_index = (y as usize * width as usize + x as usize) * 4;
            for channel in 0..4 {
                let value = sums[channel] / sample_count;
                pixels[destination_index + channel] =
                    if color_space == TextureColorSpace::Srgb && channel < 3 {
                        linear_to_srgb(value)
                    } else {
                        (value * 255.0).round() as u8
                    };
            }
        }
    }
    pixels
}

fn srgb_to_linear(value: u8) -> f32 {
    static LOOKUP: OnceLock<[f32; 256]> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        std::array::from_fn(|value| {
            let value = value as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    })[value as usize]
}

fn linear_to_srgb(value: f32) -> u8 {
    static LOOKUP: OnceLock<[u8; 65_536]> = OnceLock::new();
    let lookup = LOOKUP.get_or_init(|| {
        std::array::from_fn(|value| {
            let value = value as f32 / 65_535.0;
            let value = if value <= 0.0031308 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    });
    lookup[(value.clamp(0.0, 1.0) * 65_535.0).round() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_validate_rgba8_data_and_material_defaults_are_rough_dielectrics() {
        assert!(matches!(
            Texture::new(2, 2, vec![0; 3]),
            Err(TextureError::InvalidData {
                actual: 3,
                expected: 16
            })
        ));
        let texture = Texture::linear(1, 1, vec![128, 128, 255, 255]).unwrap();
        assert_eq!(texture.color_space, TextureColorSpace::Linear);

        let material = Material::default();
        assert_eq!(material.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(material.metallic, 0.0);
        assert_eq!(material.roughness, 0.5);
        assert!(material.textures.base_color.is_none());
        assert!(material.textures.normal.is_none());
        assert!(material.textures.metallic_roughness.is_none());
        assert!(material.textures.emissive.is_none());
    }

    #[test]
    fn material_helpers_configure_a_texture_set() {
        let texture = TextureHandle(7);
        let material = Material::textured(texture)
            .with_normal_texture(TextureHandle(8))
            .with_metallic_roughness_texture(TextureHandle(9))
            .with_emissive_texture(TextureHandle(10));

        assert_eq!(
            material.textures,
            TextureSet {
                base_color: Some(texture),
                normal: Some(TextureHandle(8)),
                metallic_roughness: Some(TextureHandle(9)),
                emissive: Some(TextureHandle(10)),
            }
        );
    }

    #[test]
    fn texture_mip_levels_downsample_odd_dimensions_and_retain_alpha() {
        let texture = Texture::linear(
            3,
            2,
            vec![
                0, 10, 20, 30, 10, 20, 30, 40, 20, 30, 40, 50, 30, 40, 50, 60, 40, 50, 60, 70, 50,
                60, 70, 80,
            ],
        )
        .unwrap();

        assert_eq!(texture.mip_level_count(), 2);
        let levels = texture.mip_levels().unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!((levels[1].width, levels[1].height), (1, 1));
        assert_eq!(levels[1].pixels, vec![25, 35, 45, 55]);
    }

    #[test]
    fn srgb_mip_levels_average_rgb_in_linear_space() {
        let texture = Texture::new(2, 1, vec![0, 0, 0, 0, 255, 255, 255, 255]).unwrap();

        let levels = texture.mip_levels().unwrap();
        assert_eq!(levels[1].pixels, vec![188, 188, 188, 128]);
    }

    #[test]
    fn material_slot_from_normal_selects_the_dominant_axis() {
        assert_eq!(
            MaterialSlot::from_normal([0.0, 1.0, 0.0]),
            MaterialSlot::Top
        );
        assert_eq!(
            MaterialSlot::from_normal([0.0, -1.0, 0.0]),
            MaterialSlot::Bottom
        );
        assert_eq!(
            MaterialSlot::from_normal([0.0, 0.0, 1.0]),
            MaterialSlot::Front
        );
        assert_eq!(
            MaterialSlot::from_normal([0.0, 0.0, -1.0]),
            MaterialSlot::Back
        );
        assert_eq!(
            MaterialSlot::from_normal([1.0, 0.0, 0.0]),
            MaterialSlot::Right
        );
        assert_eq!(
            MaterialSlot::from_normal([-1.0, 0.0, 0.0]),
            MaterialSlot::Left
        );
        assert_eq!(
            MaterialSlot::from_normal([0.0, 0.0, 0.0]),
            MaterialSlot::Top
        );
    }

    #[test]
    fn material_slot_from_normal_prefers_up_on_ties() {
        // Wedge slopes face up and sideways equally; they should read as top
        // faces so a grass-top style override covers them.
        assert_eq!(
            MaterialSlot::from_normal([
                -std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
                0.0
            ]),
            MaterialSlot::Top
        );
    }

    #[test]
    fn mesh_material_slots_round_trip_through_get() {
        let mut slots = MeshMaterialSlots::default();
        assert!(slots.get(MaterialSlot::Top).is_none());

        let material = Material::from_color(Color3::new(1.0, 0.0, 0.0));
        slots.set(MaterialSlot::Top, material);
        assert_eq!(slots.get(MaterialSlot::Top), Some(&material));
        assert!(slots.get(MaterialSlot::Bottom).is_none());
    }

    #[test]
    fn mesh_material_slots_leave_skipped_slots_unset() {
        let mut slots = MeshMaterialSlots::default();
        let right = Material::from_color(Color3::new(0.0, 1.0, 0.0));
        let bottom = Material::from_color(Color3::new(0.0, 0.0, 1.0));
        slots.set(MaterialSlot::Right, right);
        slots.set(MaterialSlot::Bottom, bottom);
        assert!(slots.get(MaterialSlot::Top).is_none());
        assert_eq!(slots.get(MaterialSlot::Bottom), Some(&bottom));
        assert_eq!(slots.get(MaterialSlot::Right), Some(&right));
    }

    #[test]
    fn texture_pixel_access_reads_and_writes_texels() {
        let mut texture = Texture::linear(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(texture.pixel(0, 0), [1, 2, 3, 4]);
        assert_eq!(texture.pixel(1, 0), [5, 6, 7, 8]);

        texture.set_pixel(1, 0, [9, 9, 9, 9]);
        assert_eq!(texture.pixel(1, 0), [9, 9, 9, 9]);

        texture.pixels_mut()[0] = 42;
        assert_eq!(texture.pixels()[0], 42);
        assert_eq!(texture.pixel(0, 0), [42, 2, 3, 4]);
    }

    #[test]
    fn texture_flips_mirror_texel_positions() {
        // Row-major 2x2: A B / C D.
        let pixels: Vec<u8> = [
            [10, 0, 0, 255],
            [20, 0, 0, 255],
            [30, 0, 0, 255],
            [40, 0, 0, 255],
        ]
        .into_iter()
        .flatten()
        .collect();

        let mut texture = Texture::linear(2, 2, pixels.clone()).unwrap();
        texture = texture.flip_horizontal();
        assert_eq!(texture.pixel(0, 0)[0], 20);
        assert_eq!(texture.pixel(1, 0)[0], 10);
        assert_eq!(texture.pixel(0, 1)[0], 40);
        assert_eq!(texture.pixel(1, 1)[0], 30);
        texture = texture.flip_horizontal();
        assert_eq!(texture.pixels(), &pixels);

        let mut texture = Texture::linear(2, 2, pixels.clone()).unwrap();
        texture = texture.flip_vertical();
        assert_eq!(texture.pixel(0, 0)[0], 30);
        assert_eq!(texture.pixel(0, 1)[0], 10);
        texture = texture.flip_vertical();
        assert_eq!(texture.pixels(), &pixels);
    }

    #[test]
    fn texture_rotations_move_texels_and_swap_dimensions() {
        // Row-major 2x1: A B.
        let pixels: Vec<u8> = [[10, 0, 0, 255], [20, 0, 0, 255]]
            .into_iter()
            .flatten()
            .collect();

        let texture = Texture::linear(2, 1, pixels.clone()).unwrap().rotate90();
        assert_eq!((texture.width, texture.height), (1, 2));
        // Clockwise: the left texel swings to the top.
        assert_eq!(texture.pixel(0, 0)[0], 10);
        assert_eq!(texture.pixel(0, 1)[0], 20);
        let texture = texture.rotate270();
        assert_eq!((texture.width, texture.height), (2, 1));
        assert_eq!(texture.pixels(), &pixels);

        let texture = Texture::linear(2, 1, pixels.clone()).unwrap().rotate270();
        assert_eq!((texture.width, texture.height), (1, 2));
        // Counter-clockwise: the right texel swings to the top.
        assert_eq!(texture.pixel(0, 0)[0], 20);
        assert_eq!(texture.pixel(0, 1)[0], 10);

        let texture = Texture::linear(2, 1, pixels.clone()).unwrap().rotate180();
        assert_eq!((texture.width, texture.height), (2, 1));
        assert_eq!(texture.pixel(0, 0)[0], 20);
        assert_eq!(texture.pixel(1, 0)[0], 10);
        let texture = texture.rotate180();
        assert_eq!(texture.pixels(), &pixels);

        let mut texture = Texture::linear(2, 1, pixels.clone()).unwrap();
        for _ in 0..4 {
            texture = texture.rotate90();
        }
        assert_eq!((texture.width, texture.height), (2, 1));
        assert_eq!(texture.pixels(), &pixels);
    }

    #[test]
    fn texture_color_adjustments_scale_and_replace_channels() {
        let texture = Texture::linear(1, 1, vec![200, 100, 50, 255])
            .unwrap()
            .tint([0.5, 1.0, 2.0, 1.0]);
        assert_eq!(texture.pixel(0, 0), [100, 100, 100, 255]);

        let texture = Texture::linear(1, 1, vec![255, 0, 0, 128])
            .unwrap()
            .grayscale();
        let gray = texture.pixel(0, 0);
        assert_eq!(gray, [54, 54, 54, 128]);

        let texture = Texture::linear(1, 1, vec![10, 20, 30, 40])
            .unwrap()
            .invert_rgb();
        assert_eq!(texture.pixel(0, 0), [245, 235, 225, 40]);
    }

    #[test]
    fn image_transforms_mirror_the_texture_api() {
        let mut image = Image::from_rgba8(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(image.pixel(1, 0), [5, 6, 7, 8]);
        image.set_pixel(0, 0, [9, 9, 9, 9]);
        assert_eq!(image.pixel(0, 0), [9, 9, 9, 9]);

        image = image.flip_horizontal().flip_vertical();
        assert_eq!(image.pixel(0, 0), [5, 6, 7, 8]);
        assert_eq!(image.pixel(1, 0), [9, 9, 9, 9]);

        image = image.rotate90();
        assert_eq!((image.width, image.height), (1, 2));
        image = image.rotate270();
        assert_eq!((image.width, image.height), (2, 1));

        image = image.rotate180().rotate180();
        image = image.tint([1.0, 1.0, 1.0, 1.0]).grayscale().invert_rgb();
        assert_eq!(image.pixels().len(), 8);
    }
}
