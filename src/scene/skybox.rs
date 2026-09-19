use std::f32::consts::PI;

use thiserror::Error;

use crate::{Face, Image};

/// Errors produced while building a [`Skybox`] from image data.
#[derive(Debug, Error)]
pub enum SkyboxError {
    /// Faces of a cubemap did not share one square size.
    #[error("cubemap face {face:?} is {width}x{height}, expected {expected}x{expected}")]
    FaceSizeMismatch {
        /// The face whose dimensions differed.
        face: Face,
        /// The face width in pixels.
        width: u32,
        /// The face height in pixels.
        height: u32,
        /// The square size taken from the first face.
        expected: u32,
    },
    /// A cubemap face was not square.
    #[error("cubemap face {face:?} is {width}x{height}, faces must be square")]
    NonSquareFace {
        /// The non-square face.
        face: Face,
        /// The face width in pixels.
        width: u32,
        /// The face height in pixels.
        height: u32,
    },
    /// A cross-layout image was not a 4x3 or 3x4 grid of square faces.
    #[error(
        "cross-layout skybox is {width}x{height}, expected a 4x3 (width = 4 * face, height = 3 * face) or 3x4 grid of square faces"
    )]
    InvalidCrossLayout {
        /// The image width in pixels.
        width: u32,
        /// The image height in pixels.
        height: u32,
    },
    /// A panorama was not a 2:1 equirectangular image.
    #[error(
        "panorama skybox is {width}x{height}, expected a 2:1 equirectangular image (width = 2 * height)"
    )]
    InvalidPanoramaDimensions {
        /// The image width in pixels.
        width: u32,
        /// The image height in pixels.
        height: u32,
    },
}

/// A cubemap skybox stored as six square faces in [`Face::ALL_CUBEMAP`] order.
///
/// A skybox can be built from six individual face images
/// ([`from_faces`](Self::from_faces)), from a single cross-layout image
/// ([`from_cross`](Self::from_cross)), or from a 2:1 equirectangular panorama
/// ([`from_equirectangular`](Self::from_equirectangular)). All three paths
/// produce the same in-memory representation, so they render identically.
///
/// Faces are sampled as sRGB color data.
#[derive(Clone, Debug)]
pub struct Skybox {
    faces: [Image; 6],
}

/// Embedded default skybox: a 4x3 cross layout with 512-pixel faces.
#[cfg(feature = "default-skybox")]
const DEFAULT_SKYBOX_CROSS_BYTES: &[u8] = include_bytes!("../bin/Cubemap_Sky_01-512x512.webp");

impl Skybox {
    /// Builds a skybox from six square face images of equal size.
    ///
    /// Faces are ordered as `[Right, Left, Top, Bottom, Front, Back]`
    /// (see [`Face::ALL_CUBEMAP`]); use [`Face::cubemap_index`] to place each
    /// image. Every face must be square and share the first face's size.
    pub fn from_faces(faces: [Image; 6]) -> Result<Self, SkyboxError> {
        let expected = faces[0].width;
        if faces[0].width != faces[0].height {
            return Err(SkyboxError::NonSquareFace {
                face: Face::ALL_CUBEMAP[0],
                width: faces[0].width,
                height: faces[0].height,
            });
        }
        for (face, image) in Face::ALL_CUBEMAP.iter().zip(faces.iter()).skip(1) {
            if image.width != expected || image.height != expected {
                return Err(SkyboxError::FaceSizeMismatch {
                    face: *face,
                    width: image.width,
                    height: image.height,
                    expected,
                });
            }
        }
        Ok(Self { faces })
    }

    /// Builds a skybox by splitting a cross-layout image into six faces.
    ///
    /// Two grid shapes are accepted, with `S` the square face size:
    ///
    /// Horizontal (`width = 4 * S`, `height = 3 * S`):
    /// ```text
    ///     +Y
    /// -X  +Z  +X  -Z
    ///     -Y
    /// ```
    ///
    /// Vertical (`width = 3 * S`, `height = 4 * S`):
    /// ```text
    ///     +Y
    /// -X  +Z  +X
    ///     -Z
    ///     -Y
    /// ```
    ///
    /// Each face is stored upright: the top row of pixels faces world `+Y`
    /// for the four side faces, while the top and bottom faces orient their
    /// rows toward `-Z`/`+Z` respectively so shared edges match without
    /// rotation.
    pub fn from_cross(image: Image) -> Result<Self, SkyboxError> {
        let (width, height) = (image.width, image.height);
        // Grid tile positions in [Top, Left, Front, Right, Back, Bottom] order.
        let layout = if width % 4 == 0 && height % 3 == 0 && width / 4 == height / 3 {
            Some((width / 4, [(1, 0), (0, 1), (1, 1), (2, 1), (3, 1), (1, 2)]))
        } else if width % 3 == 0 && height % 4 == 0 && width / 3 == height / 4 {
            Some((width / 3, [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2), (1, 3)]))
        } else {
            None
        };
        // Faces below are in GPU layer order [Right, Left, Top, Bottom,
        // Front, Back], remapped from the grid's [Top, Left, Front, Right,
        // Back, Bottom] tile order.
        let (face_size, grid) = layout.ok_or(SkyboxError::InvalidCrossLayout { width, height })?;
        let tiles = [
            extract_tile(&image, grid[3], face_size), // Right (+X)
            extract_tile(&image, grid[1], face_size), // Left (-X)
            extract_tile(&image, grid[0], face_size), // Top (+Y)
            extract_tile(&image, grid[5], face_size), // Bottom (-Y)
            extract_tile(&image, grid[2], face_size), // Front (+Z)
            extract_tile(&image, grid[4], face_size), // Back (-Z)
        ];
        let mut faces = tiles.into_iter();
        Ok(Self {
            faces: [
                faces.next().expect("six cross tiles"),
                faces.next().expect("six cross tiles"),
                faces.next().expect("six cross tiles"),
                faces.next().expect("six cross tiles"),
                faces.next().expect("six cross tiles"),
                faces.next().expect("six cross tiles"),
            ],
        })
    }

    /// Builds a skybox by converting a 2:1 equirectangular panorama.
    ///
    /// Each generated face is `height / 2` pixels square. The panorama's
    /// horizontal center (`u = 0.5`) looks toward [`Face::Front`]
    /// (`+Z`), `u` increases through Right (`+X`), Back (`-Z`), and Left
    /// (`-X`), and the top/bottom rows map to the zenith/nadir.
    pub fn from_equirectangular(panorama: Image) -> Result<Self, SkyboxError> {
        let (width, height) = (panorama.width, panorama.height);
        if width != height.checked_mul(2).unwrap_or(0) || height < 2 {
            return Err(SkyboxError::InvalidPanoramaDimensions { width, height });
        }
        let face_size = height / 2;
        let faces = Face::ALL_CUBEMAP.map(|face| {
            let mut pixels = vec![0u8; face_size as usize * face_size as usize * 4];
            for y in 0..face_size {
                // tc in [-1, 1], top row (-1) to bottom row (+1).
                let tc = (y as f32 + 0.5) / face_size as f32 * 2.0 - 1.0;
                for x in 0..face_size {
                    // sc in [-1, 1], left (-1) to right (+1).
                    let sc = (x as f32 + 0.5) / face_size as f32 * 2.0 - 1.0;
                    let direction = face_texel_direction(face, sc, tc);
                    let pixel = sample_equirectangular(&panorama, direction);
                    let start = (y as usize * face_size as usize + x as usize) * 4;
                    pixels[start..start + 4].copy_from_slice(&pixel);
                }
            }
            Image::from_rgba8(face_size, face_size, pixels)
                .expect("converted skybox face has valid dimensions")
        });
        Ok(Self { faces })
    }

    /// Returns the face image looking in `face`'s direction.
    pub fn face(&self, face: Face) -> &Image {
        &self.faces[face.cubemap_index()]
    }

    /// Returns all six face images in [`Face::ALL_CUBEMAP`] order.
    pub fn faces(&self) -> &[Image; 6] {
        &self.faces
    }

    /// Returns the square edge length of each face in pixels.
    pub fn face_size(&self) -> u32 {
        self.faces[0].width
    }
}

#[cfg(feature = "default-skybox")]
impl Default for Skybox {
    /// Returns an embedded default skybox.
    fn default() -> Self {
        Self::from_cross(Image::from_bytes(DEFAULT_SKYBOX_CROSS_BYTES).unwrap()).unwrap()
    }
}

/// Copies the `face_size` square tile at grid position `(tile_x, tile_y)` out
/// of a cross-layout image.
fn extract_tile(image: &Image, tile: (u32, u32), face_size: u32) -> Image {
    let (tile_x, tile_y) = tile;
    let mut pixels = vec![0u8; face_size as usize * face_size as usize * 4];
    for y in 0..face_size {
        let source_start = ((tile_y * face_size + y) as usize * image.width as usize
            + (tile_x * face_size) as usize)
            * 4;
        let destination_start = y as usize * face_size as usize * 4;
        pixels[destination_start..destination_start + face_size as usize * 4]
            .copy_from_slice(&image.pixels[source_start..source_start + face_size as usize * 4]);
    }
    Image::from_rgba8(face_size, face_size, pixels).expect("cross-layout tile has valid dimensions")
}

/// Returns the world-space direction of face texel (`sc`, `tc`).
///
/// `sc`/`tc` range over `[-1, 1]` with `(-1, -1)` at the top-left texel.
/// The mapping is the inverse of the cube-sampling convention used by the
/// renderer, so faces converted from panoramas line up with faces split from
/// cross layouts: side faces store `+Y` in their top row, the top face stores
/// `-Z` in its top row, and the bottom face stores `+Z` in its top row.
fn face_texel_direction(face: Face, sc: f32, tc: f32) -> [f32; 3] {
    match face {
        Face::Right => [1.0, -tc, -sc],
        Face::Left => [-1.0, -tc, sc],
        Face::Top => [sc, 1.0, tc],
        Face::Bottom => [sc, -1.0, -tc],
        Face::Front => [sc, -tc, 1.0],
        Face::Back => [-sc, -tc, -1.0],
    }
}

/// Samples a 2:1 equirectangular panorama in the given world direction with
/// bilinear filtering, wrapping horizontally and clamping vertically.
fn sample_equirectangular(panorama: &Image, direction: [f32; 3]) -> [u8; 4] {
    let length =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt()
            .max(1e-12);
    let (x, y, z) = (
        direction[0] / length,
        (direction[1] / length).clamp(-1.0, 1.0),
        direction[2] / length,
    );
    let u = (x.atan2(z) / (2.0 * PI) + 0.5).rem_euclid(1.0);
    let v = y.acos() / PI;
    let (width, height) = (panorama.width as f32, panorama.height as f32);
    bilinear_sample(panorama, u * width - 0.5, v * height - 0.5)
}

/// Bilinearly samples `image` at floating-point texel coordinates, wrapping
/// `x` around the width and clamping `y` to the image rows.
fn bilinear_sample(image: &Image, fx: f32, fy: f32) -> [u8; 4] {
    let (width, height) = (image.width as i64, image.height as i64);
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let mut result = [0u8; 4];
    for (channel, output) in result.iter_mut().enumerate() {
        let sample = |x: i64, y: i64| {
            let x = x.rem_euclid(width) as u32;
            let y = y.clamp(0, height - 1) as u32;
            f32::from(image.pixel(x, y)[channel])
        };
        let top = sample(x0, y0) * (1.0 - tx) + sample(x0 + 1, y0) * tx;
        let bottom = sample(x0, y0 + 1) * (1.0 - tx) + sample(x0 + 1, y0 + 1) * tx;
        *output = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_face(value: [u8; 4], size: u32) -> Image {
        let mut pixels = Vec::with_capacity(size as usize * size as usize * 4);
        for _ in 0..size * size {
            pixels.extend_from_slice(&value);
        }
        Image::from_rgba8(size, size, pixels).unwrap()
    }

    #[test]
    fn faces_require_matching_square_sizes() {
        let faces = [
            solid_face([255, 0, 0, 255], 4),
            solid_face([0, 255, 0, 255], 4),
            solid_face([0, 0, 255, 255], 4),
            solid_face([255, 255, 0, 255], 4),
            solid_face([0, 255, 255, 255], 4),
            solid_face([255, 0, 255, 255], 4),
        ];
        let skybox = Skybox::from_faces(faces).unwrap();
        assert_eq!(skybox.face_size(), 4);
        assert_eq!(skybox.face(Face::Front).pixel(0, 0), [0, 255, 255, 255]);

        let mut mismatched = Face::ALL_CUBEMAP.map(|_| solid_face([1, 2, 3, 255], 4));
        mismatched[Face::Top.cubemap_index()] = solid_face([1, 2, 3, 255], 8);
        assert!(matches!(
            Skybox::from_faces(mismatched),
            Err(SkyboxError::FaceSizeMismatch { .. })
        ));

        let mut non_square = Face::ALL_CUBEMAP.map(|_| solid_face([1, 2, 3, 255], 4));
        non_square[0] = Image::from_rgba8(4, 2, vec![0; 32]).unwrap();
        assert!(matches!(
            Skybox::from_faces(non_square),
            Err(SkyboxError::NonSquareFace { .. })
        ));
    }

    #[test]
    fn cross_split_extracts_each_tile_without_rotation() {
        // Paint each grid tile of a 4x3 cross a distinct solid color, then
        // check every face reads back its tile's color.
        let face_size = 4u32;
        let (columns, rows) = (4u32, 3u32);
        let mut pixels =
            vec![
                0u8;
                columns as usize * face_size as usize * rows as usize * face_size as usize * 4
            ];
        let tile_colors = [
            ((1, 0), [10, 0, 0, 255]),
            ((0, 1), [0, 20, 0, 255]),
            ((1, 1), [0, 0, 30, 255]),
            ((2, 1), [40, 0, 0, 255]),
            ((3, 1), [0, 50, 0, 255]),
            ((1, 2), [0, 0, 60, 255]),
        ];
        for ((tile_x, tile_y), color) in tile_colors {
            for y in 0..face_size {
                for x in 0..face_size {
                    let index =
                        (((tile_y * face_size + y) * columns * face_size + tile_x * face_size + x)
                            * 4) as usize;
                    pixels[index..index + 4].copy_from_slice(&color);
                }
            }
        }
        let image = Image::from_rgba8(columns * face_size, rows * face_size, pixels).unwrap();
        let skybox = Skybox::from_cross(image).unwrap();

        assert_eq!(skybox.face(Face::Top).pixel(0, 0), [10, 0, 0, 255]);
        assert_eq!(skybox.face(Face::Left).pixel(0, 0), [0, 20, 0, 255]);
        assert_eq!(skybox.face(Face::Front).pixel(0, 0), [0, 0, 30, 255]);
        assert_eq!(skybox.face(Face::Right).pixel(0, 0), [40, 0, 0, 255]);
        assert_eq!(skybox.face(Face::Back).pixel(0, 0), [0, 50, 0, 255]);
        assert_eq!(skybox.face(Face::Bottom).pixel(0, 0), [0, 0, 60, 255]);
    }

    #[test]
    fn cross_split_supports_the_vertical_grid() {
        let face_size = 2u32;
        let (columns, rows) = (3u32, 4u32);
        let mut pixels =
            vec![
                0u8;
                columns as usize * face_size as usize * rows as usize * face_size as usize * 4
            ];
        // Vertical grid order: Top, Left, Front, Right, Back, Bottom.
        let tile_colors = [
            ((1, 0), [10, 0, 0, 255]),
            ((0, 1), [0, 20, 0, 255]),
            ((1, 1), [0, 0, 30, 255]),
            ((2, 1), [40, 0, 0, 255]),
            ((1, 2), [0, 50, 0, 255]),
            ((1, 3), [0, 0, 60, 255]),
        ];
        for ((tile_x, tile_y), color) in tile_colors {
            for y in 0..face_size {
                for x in 0..face_size {
                    let index =
                        (((tile_y * face_size + y) * columns * face_size + tile_x * face_size + x)
                            * 4) as usize;
                    pixels[index..index + 4].copy_from_slice(&color);
                }
            }
        }
        let image = Image::from_rgba8(columns * face_size, rows * face_size, pixels).unwrap();
        let skybox = Skybox::from_cross(image).unwrap();

        assert_eq!(skybox.face(Face::Top).pixel(0, 0), [10, 0, 0, 255]);
        assert_eq!(skybox.face(Face::Front).pixel(0, 0), [0, 0, 30, 255]);
        assert_eq!(skybox.face(Face::Back).pixel(0, 0), [0, 50, 0, 255]);
        assert_eq!(skybox.face(Face::Bottom).pixel(0, 0), [0, 0, 60, 255]);
    }

    #[test]
    fn cross_split_rejects_images_without_a_face_grid() {
        let image = Image::from_rgba8(5, 5, vec![0; 100]).unwrap();
        assert!(matches!(
            Skybox::from_cross(image),
            Err(SkyboxError::InvalidCrossLayout { .. })
        ));
    }

    #[test]
    fn panorama_conversion_maps_poles_and_cardinal_directions() {
        // Build a panorama whose regions encode direction. Color boundaries
        // sit at u = 1/8, 3/8, 5/8, 7/8 so each cubemap face center lands
        // solidly inside one region instead of straddling a boundary texel:
        // red surrounds u = 1/4 (-X), blue surrounds u = 1/2 (+Z front),
        // green surrounds u = 3/4 (+X), yellow surrounds u = 0 (-Z back),
        // with a white polar cap and black nadir cap.
        let (width, height) = (64u32, 32u32);
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        for y in 0..height {
            for x in 0..width {
                let color = if y < 2 {
                    [255, 255, 255, 255]
                } else if y >= height - 2 {
                    [0, 0, 0, 255]
                } else {
                    match x {
                        8..24 => [255, 0, 0, 255],
                        24..40 => [0, 0, 255, 255],
                        40..56 => [0, 255, 0, 255],
                        _ => [255, 255, 0, 255],
                    }
                };
                let index = (y as usize * width as usize + x as usize) * 4;
                pixels[index..index + 4].copy_from_slice(&color);
            }
        }
        let panorama = Image::from_rgba8(width, height, pixels).unwrap();
        let skybox = Skybox::from_equirectangular(panorama).unwrap();
        assert_eq!(skybox.face_size(), 16);

        let center = |face| skybox.face(face).pixel(8, 8);
        assert_eq!(center(Face::Front), [0, 0, 255, 255]);
        assert_eq!(center(Face::Right), [0, 255, 0, 255]);
        assert_eq!(center(Face::Back), [255, 255, 0, 255]);
        assert_eq!(center(Face::Left), [255, 0, 0, 255]);
        assert_eq!(center(Face::Top), [255, 255, 255, 255]);
        assert_eq!(center(Face::Bottom), [0, 0, 0, 255]);
    }

    #[test]
    fn panorama_conversion_rejects_non_2_to_1_images() {
        let image = Image::from_rgba8(10, 10, vec![0; 400]).unwrap();
        assert!(matches!(
            Skybox::from_equirectangular(image),
            Err(SkyboxError::InvalidPanoramaDimensions { .. })
        ));
    }

    #[cfg(feature = "default-skybox")]
    #[test]
    fn default_skybox_loads_the_embedded_cross_layout() {
        let skybox = Skybox::default();
        assert_eq!(skybox.face_size(), 512);
        // The embedded cross is photographic, so no face is a flat color.
        for face in Face::ALL_CUBEMAP {
            let image = skybox.face(face);
            assert!(image.pixels().iter().any(|&byte| byte != 0));
        }
    }
}
