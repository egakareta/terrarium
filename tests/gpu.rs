use terrarium::{glam::*, *};

// Software WGPU adapters share context state across the headless test cases.
static GPU_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize_gpu_test() -> std::sync::MutexGuard<'static, ()> {
    GPU_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn imported_gltf_is_rendered() -> Result<(), terrarium::AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    use terrarium::HasMaterials as _;
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|engine| {
            let mesh = engine.add_mesh(include_bytes!(
                "../crates/trscene/fixtures/DamagedHelmet.glb"
            ))?;
            engine.add_child(MeshPart::new(mesh).with_position(Vec3::new(0.0, 0.75, 0.0)));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    let meshpart = engine.get_all::<MeshPart>().next().unwrap();
    assert!(
        terrarium::Face::ALL.into_iter().any(|slot| meshpart
            .material_slot(slot)
            .textures()
            .emissive
            .is_some())
    );
    let occlusion_material = terrarium::Face::ALL
        .into_iter()
        .map(|slot| meshpart.material_slot(slot))
        .find(|material| material.occlusion_texture().is_some())
        .expect("DamagedHelmet has an occlusion map");
    assert_eq!(occlusion_material.occlusion_strength(), 1.0);
    let pixels = engine.renderer().read_pixels()?;
    let rendered_pixels = pixels
        .iter()
        .filter(|pixel| **pixel != [0, 0, 0, 255])
        .collect::<Vec<_>>();
    let rendered_pixel_count = rendered_pixels.len();
    let max_channel_difference = rendered_pixels
        .iter()
        .map(|pixel| pixel[..3].iter().max().unwrap() - pixel[..3].iter().min().unwrap())
        .max()
        .unwrap_or(0);

    assert!(rendered_pixel_count > 0);
    assert!(max_channel_difference > 32);
    Ok(())
}

#[test]
fn screen_pixels_are_readable() -> Result<(), Box<dyn std::error::Error>> {
    let _gpu_test_guard = serialize_gpu_test();
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|engine| {
            engine.lighting.clear_skybox();
            engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
            let lantern = engine.add_texture(Texture::from_bytes(
                include_bytes!("../assets/festival_lantern.png"),
                TextureColorSpace::Srgb,
            )?)?;

            engine.add_child(
                Part::new()
                    .with_name("Lantern")
                    .with_shape(PartShape::Block)
                    .with_position(Vec3::new(0.0, 1.0, 0.0))
                    .with_size(Vec3::new(2.0, 2.0, 2.0))
                    .with_material(Material::textured(lantern)),
            );
            Ok::<(), AppCreationError>(())
        })
        .unwrap()
        .unwrap();
    let renderer = engine.renderer();
    let pixels = renderer.read_pixels()?;
    let top_left = pixels[0];
    let center = pixels[64 * 128 + 64];
    let rendered_pixel_count = pixels
        .iter()
        .filter(|pixel| **pixel != [0, 0, 0, 255])
        .count();
    println!("screen pixels: top_left={top_left:?}, center={center:?}");
    assert_eq!(top_left, [0, 0, 0, 255]);
    assert!(rendered_pixel_count > 0);
    Ok(())
}

#[test]
fn multiple_viewports_keep_independent_egui_textures() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(0))
        .run(|engine| {
            let first = Viewport::new([16, 16]);
            let second = Viewport::new([32, 16]);
            let first_texture = engine.render_viewport(&first)?;
            let second_texture = engine.render_viewport(&second)?;

            assert_ne!(first_texture.id(), second_texture.id());
            assert_eq!(first_texture, engine.render_viewport(&first)?);
            assert_eq!(second_texture, engine.render_viewport(&second)?);
            Ok::<(), AppCreationError>(())
        })?
        .expect("native headless mode returns the engine");
    Ok(())
}

#[test]
fn basepart_transparency_blends_front_geometry_with_background()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _gpu_test_guard = serialize_gpu_test();
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|engine| {
            engine.lighting.clear_skybox();
            engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
            engine.workspace.current_camera =
                Camera::new(Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.0, 1.0, 0.0), 1.0);
            engine.add_child(
                Part::new()
                    .with_position(Vec3::new(0.0, 1.0, -1.0))
                    .with_size(Vec3::splat(3.0))
                    .with_color(Color3::RED),
            );
            engine.add_child(
                Part::new()
                    .with_position(Vec3::new(0.0, 1.0, 1.0))
                    .with_size(Vec3::splat(3.0))
                    .with_color(Color3::BLUE)
                    .with_transparency(0.5),
            );
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();

    let center = engine.renderer().read_pixel(64, 64)?;
    assert!(
        center[0] > 0 && center[2] > 0,
        "transparent foreground should preserve both background and foreground color: {center:?}"
    );
    Ok(())
}

#[test]
fn instance_outlines_render_in_all_modes() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    for mode in [
        OutlineMode::Toon,
        OutlineMode::Silhouette,
        OutlineMode::Stencil,
    ] {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.lighting.clear_skybox();
                engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
                let part_id = engine.add_child(
                    Part::new()
                        .with_position(Vec3::ZERO)
                        .with_size(Vec3::splat(2.0)),
                );
                engine
                    .get_mut::<Part>(part_id)
                    .expect("outline target should be in the workspace")
                    .add_child_ref(
                        Outline::new(mode)
                            .with_color(Color3::new(1.0, 0.0, 0.0))
                            .with_width(2.0),
                    );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        let pixels = engine.renderer().read_pixels()?;
        let red_pixels = pixels
            .iter()
            .filter(|pixel| pixel[0] > 180 && pixel[1] < 80 && pixel[2] < 80)
            .count();
        assert!(
            red_pixels > 0,
            "{mode:?} outline should contribute its configured color"
        );
    }
    Ok(())
}

#[test]
fn material_triplanar_projection_samples_world_space_coordinates()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _gpu_test_guard = serialize_gpu_test();
    fn center_pixel(
        projection: TextureProjection,
        normal_pixel: Option<[u8; 4]>,
    ) -> Result<[u8; 4], Box<dyn std::error::Error + Send + Sync>> {
        let texture_pixels = (0..4)
            .flat_map(|_| {
                (0..4).flat_map(|x| {
                    if x < 2 {
                        [0, 0, 255, 255]
                    } else {
                        [255, 0, 0, 255]
                    }
                })
            })
            .collect();
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.lighting.clear_skybox();
                engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
                engine.workspace.current_camera =
                    Camera::new(Vec3::new(0.13, 0.73, 5.0), Vec3::new(0.13, 0.73, 0.0), 1.0);
                let texture = engine.add_texture(Texture::new(4, 4, texture_pixels)?)?;
                let mut material = Material::textured(texture)
                    .with_filter(TextureFilter::Nearest)
                    .with_projection(projection);
                if let Some(normal_pixel) = normal_pixel {
                    let normal =
                        engine.add_texture(Texture::linear(1, 1, normal_pixel.to_vec())?)?;
                    material = material.with_normal_texture(normal);
                }
                engine.add_child(
                    Part::new()
                        .with_position(Vec3::new(0.13, 0.73, 0.0))
                        .with_size(Vec3::splat(2.0))
                        .with_material(material),
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        Ok(engine.renderer().read_pixel(64, 64)?)
    }

    let uv_pixel = center_pixel(TextureProjection::Uv, None)?;
    let triplanar_pixel = center_pixel(TextureProjection::Triplanar, None)?;
    let neutral_normal_pixel =
        center_pixel(TextureProjection::Triplanar, Some([128, 128, 255, 255]))?;
    let tilted_normal_pixel =
        center_pixel(TextureProjection::Triplanar, Some([255, 128, 128, 255]))?;

    assert!(
        uv_pixel[0] > uv_pixel[2],
        "UV projection should sample the red half of the texture: {uv_pixel:?}"
    );
    assert!(
        triplanar_pixel[2] > triplanar_pixel[0],
        "triplanar projection should sample the blue world-space texel: {triplanar_pixel:?}"
    );
    assert_eq!(
        neutral_normal_pixel, triplanar_pixel,
        "a neutral triplanar normal map should preserve the geometric normal"
    );
    assert_ne!(
        tilted_normal_pixel, triplanar_pixel,
        "a non-neutral triplanar normal map should affect lighting"
    );
    Ok(())
}

#[test]
#[cfg(feature = "default-materials")]
fn builtin_material_loads_lazily_and_renders() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    fn render(material: Material) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.lighting.clear_skybox();
                engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
                engine.add_child(
                    Part::new()
                        .with_position(Vec3::new(0.0, 1.0, 0.0))
                        .with_size(Vec3::splat(2.0))
                        .with_material(material),
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    let builtin = Material::brick();
    let factors_only = Material::default()
        .with_metallic(builtin.metallic())
        .with_roughness(builtin.roughness())
        .with_projection(builtin.projection());
    let builtin_pixels = render(builtin)?;
    let factors_only_pixels = render(factors_only)?;
    assert_ne!(builtin_pixels, factors_only_pixels);
    Ok(())
}

#[test]
fn metallic_roughness_changes_rendered_environment_reflections() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    fn render(roughness: f32) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.workspace.current_camera =
                    Camera::new(Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.0, 1.0, 0.0), 1.0);
                engine.add_child(
                    Part::new()
                        .with_shape(PartShape::Ball)
                        .with_position(Vec3::new(0.0, 1.0, 0.0))
                        .with_size(Vec3::splat(2.0))
                        .with_material(
                            Material::default()
                                .with_metallic(1.0)
                                .with_roughness(roughness),
                        ),
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    let smooth = render(0.1)?;
    let rough = render(0.9)?;
    let changed_pixels = smooth
        .iter()
        .zip(&rough)
        .filter(|(smooth, rough)| {
            smooth[..3]
                .iter()
                .zip(&rough[..3])
                .any(|(smooth, rough)| smooth.abs_diff(*rough) > 2)
        })
        .count();

    assert!(
        changed_pixels > 16,
        "changing metallic roughness should alter the reflected environment across the surface; changed {changed_pixels} pixels"
    );
    Ok(())
}

#[test]
fn occlusion_texture_changes_ambient_but_preserves_direct_light() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    fn render(occlusion: u8, outdoor_ambient: Color3) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.lighting.clear_skybox();
                engine.lighting.outdoor_ambient = outdoor_ambient;
                engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
                engine.workspace.current_camera =
                    Camera::new(Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.0, 1.0, 0.0), 1.0);
                let occlusion_map = engine.add_texture(Texture::linear(
                    1,
                    1,
                    vec![occlusion, occlusion, occlusion, 255],
                )?)?;
                engine.add_child(
                    Part::new()
                        .with_shape(PartShape::Block)
                        .with_position(Vec3::new(0.0, 1.0, 0.0))
                        .with_size(Vec3::splat(2.0))
                        .with_material(Material::default().with_occlusion_texture(occlusion_map)),
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    let unoccluded = render(255, Color3::new(1.0, 1.0, 1.0))?;
    let occluded = render(0, Color3::new(1.0, 1.0, 1.0))?;
    let changed_pixels = unoccluded
        .iter()
        .zip(&occluded)
        .filter(|(unoccluded, occluded)| {
            unoccluded[..3]
                .iter()
                .zip(&occluded[..3])
                .any(|(unoccluded, occluded)| unoccluded.abs_diff(*occluded) > 2)
        })
        .count();

    assert!(
        changed_pixels > 16,
        "changing ambient occlusion should alter indirect light across the receiver; changed {changed_pixels} pixels"
    );
    let direct_only_unoccluded = render(255, Color3::BLACK)?;
    let direct_only_occluded = render(0, Color3::BLACK)?;
    assert_eq!(direct_only_unoccluded, direct_only_occluded);
    assert!(
        direct_only_occluded
            .iter()
            .any(|pixel| pixel[..3].iter().any(|c| *c > 8))
    );
    Ok(())
}

#[test]
fn dielectric_reflectance_changes_rendered_direct_specular() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    fn render(reflectance: f32) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.lighting.clear_skybox();
                engine.lighting.ambient = Color3::BLACK;
                engine.lighting.outdoor_ambient = Color3::BLACK;
                engine.lighting.with_sun_direction(Vec3::new(0.0, 0.5, 1.0));
                engine.workspace.current_camera =
                    Camera::new(Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.0, 1.0, 0.0), 1.0);
                engine.add_child(
                    Part::new()
                        .with_shape(PartShape::Ball)
                        .with_position(Vec3::new(0.0, 1.0, 0.0))
                        .with_size(Vec3::splat(2.0))
                        .with_material(Material::default().with_reflectance(reflectance)),
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    let zero_reflectance = render(0.0)?;
    let full_reflectance = render(1.0)?;
    let changed_pixels = zero_reflectance
        .iter()
        .zip(&full_reflectance)
        .filter(|(zero, full)| {
            zero[..3]
                .iter()
                .zip(&full[..3])
                .any(|(zero, full)| zero.abs_diff(*full) > 2)
        })
        .count();

    assert!(
        changed_pixels > 16,
        "changing dielectric reflectance should alter direct specular response; changed {changed_pixels} pixels"
    );
    Ok(())
}

#[test]
fn directional_shadow_long_shadow_has_clean_edges_and_lit_floor() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    const WIDTH: u32 = 800;
    const HEIGHT: u32 = 600;
    const CASTER_HEIGHT: f32 = 1.6;
    type ScreenPoint = (f32, f32);
    type ShadowTipEdge = ([ScreenPoint; 2], ScreenPoint);

    fn projected_shadow_offset(clock_time: f32, caster_height: f32) -> Vec3 {
        let sun = Lighting::sun_direction_from_clock_time(clock_time);
        Vec3::new(-sun.x, 0.0, -sun.z) * (caster_height / sun.y)
    }

    fn diagnostic_camera(camera_offset: Vec3, clock_time: f32, caster_height: f32) -> Camera {
        let target = Vec3::new(3.2, 0.0, -2.3) + projected_shadow_offset(clock_time, caster_height);
        Camera::new(target + camera_offset, target, WIDTH as f32 / HEIGHT as f32).with_fovy(0.3)
    }

    fn render(
        softness: f32,
        shadows: bool,
        camera_offset: Vec3,
        clock_time: f32,
        caster_height: f32,
    ) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([WIDTH, HEIGHT])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.workspace.current_camera =
                    diagnostic_camera(camera_offset, clock_time, caster_height);
                engine.lighting.with_clock_time(clock_time);
                engine.lighting.shadow_softness = softness;
                engine.lighting.global_shadows = shadows;
                engine.add_child(
                    Part::new()
                        .with_position(Vec3::new(0.0, -0.1, 0.0))
                        .with_size(Vec3::new(42.0, 0.2, 42.0)),
                );
                engine.add_child(
                    Part::new()
                        .with_position(Vec3::new(3.2, caster_height * 0.5, -2.3))
                        .with_size(Vec3::new(1.5, caster_height, 1.5))
                        .with_orientation(Vec3::new(0.0, 31.0, 0.0)),
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    fn luminance(pixel: [u8; 4]) -> f32 {
        0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2])
    }

    fn save_png(path: &std::path::Path, pixels: &[[u8; 4]], width: u32, height: u32) {
        let bytes: Vec<_> = pixels.iter().flatten().copied().collect();
        image::save_buffer(path, &bytes, width, height, image::ExtendedColorType::Rgba8)
            .unwrap_or_else(|error| panic!("failed to save {}: {error}", path.display()));
    }

    fn draw_probe_line(pixels: &mut [[u8; 4]], edge: [(f32, f32); 2], color: [u8; 4]) {
        let [(x0, y0), (x1, y1)] = edge;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).ceil() as usize;
        for step in 0..=steps {
            let fraction = if steps == 0 {
                0.0
            } else {
                step as f32 / steps as f32
            };
            let x = (x0 + dx * fraction).round() as i32;
            let y = (y0 + dy * fraction).round() as i32;
            if x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32 {
                pixels[y as usize * WIDTH as usize + x as usize] = color;
            }
        }
    }

    fn draw_interior_probe_band(pixels: &mut [[u8; 4]], edge: [(f32, f32); 2], inside: (f32, f32)) {
        let [start, end] = edge.map(|(x, y)| Vec2::new(x, y));
        let tangent = (end - start).normalize_or_zero();
        let normal = Vec2::new(-tangent.y, tangent.x);
        let midpoint = (start + end) * 0.5;
        let inside_sign = (Vec2::new(inside.0, inside.1) - midpoint)
            .dot(normal)
            .signum();
        for (offset, color) in [(16.0, [255, 255, 0, 255]), (96.0, [255, 128, 0, 255])] {
            let inset_edge = [start, end].map(|point| {
                let point = point + normal * (inside_sign * offset);
                (point.x, point.y)
            });
            draw_probe_line(pixels, inset_edge, color);
        }
    }

    fn dusk_shadow_long_side(
        camera_offset: Vec3,
        clock_time: f32,
        caster_height: f32,
    ) -> ([(f32, f32); 2], (f32, f32)) {
        let rotation = 31.0_f32.to_radians();
        let (sin, cos) = rotation.sin_cos();
        let cube_axis = Vec3::new(cos, 0.0, -sin);
        let cap_axis = Vec3::new(sin, 0.0, cos);
        let shadow_offset = projected_shadow_offset(clock_time, caster_height);
        let side_normal = Vec3::new(-shadow_offset.z, 0.0, shadow_offset.x).normalize();
        let cube_sign = cube_axis.dot(side_normal).signum();
        let cap_sign = cap_axis.dot(side_normal).signum();
        let base_corner = Vec3::new(3.2, 0.0, -2.3)
            + cube_axis * (cube_sign * 0.75)
            + cap_axis * (cap_sign * 0.75);
        let edge_midpoint = base_corner + shadow_offset * 0.5;
        let camera = diagnostic_camera(camera_offset, clock_time, caster_height);
        let view_projection = camera.view_projection_matrix();
        let to_screen = |point: Vec3| {
            let ndc = view_projection.project_point3(point);
            (
                (ndc.x * 0.5 + 0.5) * WIDTH as f32,
                (0.5 - ndc.y * 0.5) * HEIGHT as f32,
            )
        };
        (
            [base_corner, base_corner + shadow_offset].map(to_screen),
            to_screen(edge_midpoint - side_normal * 0.1),
        )
    }

    fn dusk_shadow_tip_edges(
        camera_offset: Vec3,
        clock_time: f32,
        caster_height: f32,
    ) -> [ShadowTipEdge; 2] {
        let rotation = 31.0_f32.to_radians();
        let (sin, cos) = rotation.sin_cos();
        let cube_axis = Vec3::new(cos, 0.0, -sin);
        let cap_axis = Vec3::new(sin, 0.0, cos);
        let shadow_offset = projected_shadow_offset(clock_time, caster_height);
        let shadow_direction = shadow_offset.normalize();
        let cube_sign = cube_axis.dot(shadow_direction).signum();
        let cap_sign = cap_axis.dot(shadow_direction).signum();
        let base_center = Vec3::new(3.2, 0.0, -2.3);
        let far_corner = base_center
            + shadow_offset
            + cube_axis * (cube_sign * 0.75)
            + cap_axis * (cap_sign * 0.75);
        let camera = diagnostic_camera(camera_offset, clock_time, caster_height);
        let view_projection = camera.view_projection_matrix();
        let to_screen = |point: Vec3| {
            let ndc = view_projection.project_point3(point);
            (
                (ndc.x * 0.5 + 0.5) * WIDTH as f32,
                (0.5 - ndc.y * 0.5) * HEIGHT as f32,
            )
        };
        [
            (
                [far_corner, far_corner - cube_axis * (cube_sign * 1.5)].map(to_screen),
                to_screen(far_corner - cap_axis * (cap_sign * 0.25)),
            ),
            (
                [far_corner, far_corner - cap_axis * (cap_sign * 1.5)].map(to_screen),
                to_screen(far_corner - cube_axis * (cube_sign * 0.25)),
            ),
        ]
    }

    fn luminance_at(pixels: &[[u8; 4]], point: Vec2) -> Option<f32> {
        let x0 = point.x.floor() as i32;
        let y0 = point.y.floor() as i32;
        if x0 < 0 || y0 < 0 || x0 + 1 >= WIDTH as i32 || y0 + 1 >= HEIGHT as i32 {
            return None;
        }
        let (tx, ty) = (point.x - x0 as f32, point.y - y0 as f32);
        let pixel = |x: i32, y: i32| luminance(pixels[(y as u32 * WIDTH + x as u32) as usize]);
        let top = pixel(x0, y0) * (1.0 - tx) + pixel(x0 + 1, y0) * tx;
        let bottom = pixel(x0, y0 + 1) * (1.0 - tx) + pixel(x0 + 1, y0 + 1) * tx;
        Some(top * (1.0 - ty) + bottom * ty)
    }

    fn shadow_strength_at(lit: &[[u8; 4]], shadowed: &[[u8; 4]], point: Vec2) -> Option<f32> {
        let lit_value = luminance_at(lit, point)?;
        if lit_value < 8.0 {
            return None;
        }
        Some(((lit_value - luminance_at(shadowed, point)?) / lit_value).clamp(0.0, 1.0))
    }

    fn shadow_edge_strength_jump(
        lit: &[[u8; 4]],
        shadowed: &[[u8; 4]],
        edge: [ScreenPoint; 2],
    ) -> f32 {
        let [start, end] = edge.map(|(x, y)| Vec2::new(x, y));
        let length = start.distance(end);
        let tangent = (end - start) / length;
        let normal = Vec2::new(-tangent.y, tangent.x);
        let margin = 32.0_f32.max(length * 0.1);
        let mut largest_jump = 0.0_f32;
        for inset in -8..=8 {
            let mut previous: Option<f32> = None;
            for step in 0..(length - 2.0 * margin).max(0.0) as usize {
                let point =
                    start + tangent * (margin + step as f32) + normal * (inset as f32 * 2.0);
                let strength = shadow_strength_at(lit, shadowed, point);
                if let (Some(current), Some(previous)) = (strength, previous) {
                    largest_jump = largest_jump.max((current - previous).abs());
                }
                previous = strength;
            }
        }
        largest_jump
    }

    fn shadow_edge_metrics(
        lit: &[[u8; 4]],
        shadowed: &[[u8; 4]],
        edge: [(f32, f32); 2],
        inside: (f32, f32),
    ) -> Option<(f32, f32, usize, f32, f32, usize)> {
        let [start, end] = edge.map(|(x, y)| Vec2::new(x, y));
        let edge_vector = end - start;
        let edge_length = edge_vector.length();
        if edge_length < 8.0 {
            return None;
        }
        let tangent = edge_vector / edge_length;
        let normal = Vec2::new(-tangent.y, tangent.x);
        let midpoint = (start + end) * 0.5;
        let inside_sign = (Vec2::new(inside.0, inside.1) - midpoint)
            .dot(normal)
            .signum();
        if inside_sign == 0.0 {
            return None;
        }

        let mut residuals = Vec::new();
        let mut transition_widths = Vec::new();
        const EDGE_THRESHOLD: f32 = 0.25;
        const TRANSITION_THRESHOLDS: [f32; 3] = [EDGE_THRESHOLD, 0.2, 0.05];
        const SEARCH_RADIUS: f32 = 16.0;
        const SEARCH_STEP: f32 = 0.5;
        let sample_count = edge_length.floor() as usize;
        let search_steps = (SEARCH_RADIUS * 2.0 / SEARCH_STEP) as usize;
        for sample in 1..sample_count {
            let fraction = sample as f32 / sample_count as f32;
            let edge_point = start + edge_vector * fraction;
            let mut previous_offset = inside_sign * SEARCH_RADIUS;
            let mut previous_strength =
                shadow_strength_at(lit, shadowed, edge_point + normal * previous_offset);
            if previous_strength.is_none() {
                continue;
            }
            let mut crossings = [None; TRANSITION_THRESHOLDS.len()];
            for step in 1..=search_steps {
                let offset = inside_sign * (SEARCH_RADIUS - step as f32 * SEARCH_STEP);
                let Some(strength) =
                    shadow_strength_at(lit, shadowed, edge_point + normal * offset)
                else {
                    previous_offset = offset;
                    previous_strength = None;
                    continue;
                };
                let Some(previous_value) = previous_strength else {
                    previous_offset = offset;
                    previous_strength = Some(strength);
                    continue;
                };
                for (index, threshold) in TRANSITION_THRESHOLDS.iter().enumerate() {
                    if crossings[index].is_none()
                        && previous_value > *threshold
                        && strength <= *threshold
                    {
                        let fraction = (previous_value - threshold) / (previous_value - strength);
                        crossings[index] =
                            Some(previous_offset + (offset - previous_offset) * fraction);
                    }
                }
                if crossings.iter().all(Option::is_some) {
                    break;
                }
                previous_offset = offset;
                previous_strength = Some(strength);
            }
            if let Some(offset) = crossings[0] {
                residuals.push((sample, offset));
            }
            if let (Some(high), Some(low)) = (crossings[1], crossings[2]) {
                transition_widths.push((high - low).abs());
            }
        }
        if sample_count < 8 || residuals.len() * 4 < (sample_count - 1) * 3 {
            return None;
        }
        if transition_widths.len() * 4 < (sample_count - 1) * 3 {
            return None;
        }
        let mean =
            residuals.iter().map(|(_, residual)| residual).sum::<f32>() / residuals.len() as f32;
        // Receiver normal bias shifts the whole contour; center residuals to measure edge jitter.
        let rms = (residuals
            .iter()
            .map(|(_, residual)| (residual - mean).powi(2))
            .sum::<f32>()
            / residuals.len() as f32)
            .sqrt();
        let second_differences: Vec<_> = residuals
            .windows(3)
            .filter(|window| window[0].0 + 1 == window[1].0 && window[1].0 + 1 == window[2].0)
            .map(|window| window[0].1 - 2.0 * window[1].1 + window[2].1)
            .collect();
        if second_differences.is_empty() {
            return None;
        }
        let roughness = (second_differences
            .iter()
            .map(|difference| difference.powi(2))
            .sum::<f32>()
            / second_differences.len() as f32)
            .sqrt();
        transition_widths.sort_by(f32::total_cmp);
        let median_transition_width = transition_widths[transition_widths.len() / 2];
        let mut largest_plateau = 0;
        let mut step_count = 0;
        for pair in residuals.windows(2) {
            if pair[0].0 + 1 == pair[1].0 && (pair[1].1 - pair[0].1).abs() >= 0.5 {
                step_count += 1;
            }
        }
        // Constant normal residuals describe a straight edge, not a screen-axis stair tread.
        let ordinate = |(sample, offset): (usize, f32)| {
            let point =
                start + edge_vector * (sample as f32 / sample_count as f32) + normal * offset;
            if tangent.x.abs() >= tangent.y.abs() {
                point.y
            } else {
                point.x
            }
        };
        for (index, first) in residuals.iter().copied().enumerate() {
            let mut minimum = ordinate(first);
            let mut maximum = minimum;
            let mut previous_sample = first.0;
            for next in residuals[index + 1..].iter().copied() {
                minimum = minimum.min(ordinate(next));
                maximum = maximum.max(ordinate(next));
                if next.0 != previous_sample + 1 || maximum - minimum > 0.25 {
                    break;
                }
                largest_plateau = largest_plateau.max(next.0 - first.0);
                previous_sample = next.0;
            }
        }
        Some((
            rms,
            roughness,
            residuals.len(),
            median_transition_width,
            largest_plateau as f32,
            step_count,
        ))
    }

    fn lit_floor_shadow_strengths(
        strength_image: &[[u8; 4]],
        camera_offset: Vec3,
        clock_time: f32,
        caster_height: f32,
    ) -> Vec<u8> {
        fn cross(start: Vec2, end: Vec2, point: Vec2) -> f32 {
            (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x)
        }

        fn convex_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
            points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
            points.dedup_by(|a, b| (*a - *b).length_squared() < 1e-8);
            let mut lower = Vec::new();
            for point in points.iter().copied() {
                while lower.len() >= 2
                    && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
                {
                    lower.pop();
                }
                lower.push(point);
            }
            let mut upper = Vec::new();
            for point in points.into_iter().rev() {
                while upper.len() >= 2
                    && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
                {
                    upper.pop();
                }
                upper.push(point);
            }
            lower.pop();
            upper.pop();
            lower.extend(upper);
            lower
        }

        fn inside_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
            let mut positive = false;
            let mut negative = false;
            for index in 0..polygon.len() {
                let side = cross(polygon[index], polygon[(index + 1) % polygon.len()], point);
                positive |= side > 0.0;
                negative |= side < 0.0;
                if positive && negative {
                    return false;
                }
            }
            true
        }

        fn edge_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
            let edge = end - start;
            let fraction = ((point - start).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
            point.distance(start + edge * fraction)
        }

        let rotation = 31.0_f32.to_radians();
        let (sin, cos) = rotation.sin_cos();
        let cube_axis = Vec3::new(cos, 0.0, -sin);
        let cap_axis = Vec3::new(sin, 0.0, cos);
        let base_center = Vec3::new(3.2, 0.0, -2.3);
        let shadow_offset = projected_shadow_offset(clock_time, caster_height);
        let mut shadow_vertices = Vec::with_capacity(8);
        for cube_sign in [-1.0, 1.0] {
            for cap_sign in [-1.0, 1.0] {
                let base_corner =
                    base_center + cube_axis * (cube_sign * 0.75) + cap_axis * (cap_sign * 0.75);
                shadow_vertices.push(Vec2::new(base_corner.x, base_corner.z));
                let far_corner = base_corner + shadow_offset;
                shadow_vertices.push(Vec2::new(far_corner.x, far_corner.z));
            }
        }
        let shadow_polygon = convex_hull(shadow_vertices);

        let target = base_center + shadow_offset;
        let camera_position = target + camera_offset;
        let forward = (target - camera_position).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let camera_up = right.cross(forward);
        let aspect = WIDTH as f32 / HEIGHT as f32;
        let focal_length = 1.0 / 0.15_f32.tan();
        let mut strengths = Vec::new();
        for y in 0..HEIGHT {
            let ndc_y = 1.0 - 2.0 * (y as f32 + 0.5) / HEIGHT as f32;
            for x in 0..WIDTH {
                let ndc_x = 2.0 * (x as f32 + 0.5) / WIDTH as f32 - 1.0;
                let ray = forward
                    + right * (ndc_x * aspect / focal_length)
                    + camera_up * (ndc_y / focal_length);
                if ray.y >= -1e-6 {
                    continue;
                }
                let point = camera_position + ray * (-camera_position.y / ray.y);
                if point.x.abs() > 20.5 || point.z.abs() > 20.5 {
                    continue;
                }
                let floor_point = Vec2::new(point.x, point.z);
                if inside_polygon(floor_point, &shadow_polygon) {
                    continue;
                }
                let distance = (0..shadow_polygon.len())
                    .map(|index| {
                        edge_distance(
                            floor_point,
                            shadow_polygon[index],
                            shadow_polygon[(index + 1) % shadow_polygon.len()],
                        )
                    })
                    .fold(f32::INFINITY, f32::min);
                if distance <= 0.15 {
                    continue;
                }
                strengths.push(strength_image[(y * WIDTH + x) as usize][0]);
            }
        }
        strengths
    }

    const SOFTNESS: f32 = 0.2;
    let clock_time = 17.5;
    let camera_offset = Vec3::new(-3.2, 3.2, 1.5);
    let lit = render(0.0, false, camera_offset, clock_time, CASTER_HEIGHT)?;
    let shadow = render(SOFTNESS, true, camera_offset, clock_time, CASTER_HEIGHT)?;
    let tip_edges = dusk_shadow_tip_edges(camera_offset, clock_time, CASTER_HEIGHT);
    let largest_strength_jump = tip_edges
        .iter()
        .map(|(edge, _)| shadow_edge_strength_jump(&lit, &shadow, *edge))
        .fold(0.0_f32, f32::max);
    eprintln!("shadow edge: largest along-edge strength jump {largest_strength_jump:.4} per pixel");

    let output_dir = std::env::var_os("TERRARIUM_SHADOW_TEST_OUTPUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("terrarium-shadow-test"));
    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", output_dir.display()));
    save_png(&output_dir.join("lit.png"), &lit, WIDTH, HEIGHT);
    save_png(&output_dir.join("shadow.png"), &shadow, WIDTH, HEIGHT);

    let strength: Vec<_> = lit
        .iter()
        .zip(&shadow)
        .map(|(lit_pixel, shadow_pixel)| {
            let lit_luminance = luminance(*lit_pixel);
            let value = if lit_luminance >= 8.0 {
                ((lit_luminance - luminance(*shadow_pixel)) / lit_luminance).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let channel = (value * 255.0).round() as u8;
            [channel, channel, channel, 255]
        })
        .collect();
    save_png(
        &output_dir.join("shadow-strength.png"),
        &strength,
        WIDTH,
        HEIGHT,
    );

    let mut comparison = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 3 * 4);
    for row in 0..HEIGHT as usize {
        let row_start = row * WIDTH as usize;
        let row_end = row_start + WIDTH as usize;
        for panel in [&lit, &shadow, &strength] {
            comparison.extend(panel[row_start..row_end].iter().flatten().copied());
        }
    }
    save_png(
        &output_dir.join("comparison.png"),
        &comparison
            .chunks_exact(4)
            .map(|rgba| [rgba[0], rgba[1], rgba[2], rgba[3]])
            .collect::<Vec<_>>(),
        WIDTH * 3,
        HEIGHT,
    );

    let mut probes = shadow.clone();
    for (index, (edge, inside)) in tip_edges.iter().enumerate() {
        draw_probe_line(
            &mut probes,
            *edge,
            if index == 0 {
                [0, 255, 255, 255]
            } else {
                [255, 0, 255, 255]
            },
        );
        draw_interior_probe_band(&mut probes, *edge, *inside);
    }
    save_png(&output_dir.join("tip-probes.png"), &probes, WIDTH, HEIGHT);
    eprintln!("shadow regression images: {}", output_dir.display());

    let (long_side, long_inside) = dusk_shadow_long_side(camera_offset, clock_time, CASTER_HEIGHT);
    if let Some((long_side_jitter, _, _, _, _, _)) =
        shadow_edge_metrics(&lit, &shadow, long_side, long_inside)
    {
        assert!(
            long_side_jitter < 2.0,
            "long-side jitter: {long_side_jitter:.2}px"
        );
    }

    let tip_edge_metrics: Vec<_> = tip_edges
        .iter()
        .filter_map(|(edge, inside)| shadow_edge_metrics(&lit, &shadow, *edge, *inside))
        .collect();
    assert!(
        tip_edge_metrics.len() == tip_edges.len(),
        "both long-shadow tip edges should expose measurable shade transitions"
    );
    let largest_tip_jitter = tip_edge_metrics
        .iter()
        .map(|(jitter, _, _, _, _, _)| *jitter)
        .fold(0.0, f32::max);
    let largest_tip_roughness = tip_edge_metrics
        .iter()
        .map(|(_, roughness, _, _, _, _)| *roughness)
        .fold(0.0, f32::max);
    let widest_tip_transition = tip_edge_metrics
        .iter()
        .map(|(_, _, _, width, _, _)| *width)
        .fold(0.0, f32::max);
    let longest_tip_tread = tip_edge_metrics
        .iter()
        .map(|(_, _, _, _, tread, steps)| if *steps >= 3 { *tread } else { 0.0 })
        .fold(0.0, f32::max);
    let mut lit_floor_strengths =
        lit_floor_shadow_strengths(&strength, camera_offset, clock_time, CASTER_HEIGHT);
    assert!(
        !lit_floor_strengths.is_empty(),
        "the diagnostic view should include lit ground outside the cast shadow"
    );
    lit_floor_strengths.sort_unstable();
    let lit_floor_p95 = lit_floor_strengths[(lit_floor_strengths.len() - 1) * 95 / 100];
    eprintln!(
        "shadow contour: max deviation {largest_tip_jitter:.2}px, max step roughness {largest_tip_roughness:.2}px, longest flat tread {longest_tip_tread:.1}px, widest 5–20% transition {widest_tip_transition:.2}px; lit-floor P95 {lit_floor_p95}/255 across {} samples",
        lit_floor_strengths.len()
    );
    assert!(
        largest_strength_jump <= 0.05,
        "a straight shadow edge should not have shade jumps above 5% per pixel along its length; measured {largest_strength_jump:.4}"
    );
    assert!(
        largest_tip_jitter < 0.5,
        "long-shadow tip contour should stay within 0.5 px RMS deviation; measured {largest_tip_jitter:.2}px"
    );
    assert!(
        longest_tip_tread < 8.0,
        "long-shadow tip should not have flat stair treads of 8 px or more; measured {longest_tip_tread:.1}px"
    );
    assert!(
        widest_tip_transition < 13.0,
        "long-shadow tip transition should remain localized; measured {widest_tip_transition:.2}px"
    );
    assert!(
        lit_floor_p95 <= 8,
        "lit ground outside the projected shadow should not show acne; 95th percentile attenuation was {lit_floor_p95}/255"
    );
    for probe_offset in [
        camera_offset + Vec3::new(0.01, 0.0, 0.0),
        camera_offset * 1.2,
        camera_offset * 1.3,
    ] {
        let lit = render(0.0, false, probe_offset, clock_time, CASTER_HEIGHT)?;
        let shadow = render(SOFTNESS, true, probe_offset, clock_time, CASTER_HEIGHT)?;
        for (edge, inside) in dusk_shadow_tip_edges(probe_offset, clock_time, CASTER_HEIGHT) {
            let (jitter, _, _, width, _, _) = shadow_edge_metrics(&lit, &shadow, edge, inside)
                .expect("the shifted view should expose the shadow-tip transition");
            eprintln!(
                "shadow contour at camera offset {probe_offset:?}: deviation {jitter:.2}px, transition {width:.2}px"
            );
            assert!(
                jitter < 0.5,
                "moving the camera should preserve a subpixel shadow contour; measured {jitter:.2}px at {probe_offset:?}"
            );
            assert!(
                width < 13.0,
                "moving the camera should not hide edge artifacts with blur; measured {width:.2}px at {probe_offset:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn point_light_shadows_change_occlusion_on_receivers() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    fn render(shadows: bool) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.workspace.current_camera =
                    Camera::new(Vec3::new(4.0, 4.0, 6.0), Vec3::new(0.0, 0.0, 0.0), 1.0);
                engine.lighting.clear_skybox();
                engine.lighting.brightness = 0.0;
                engine.lighting.ambient = Color3::BLACK;
                engine.lighting.outdoor_ambient = Color3::BLACK;
                engine.lighting.global_shadows = false;
                engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
                engine.add_child(
                    Part::new()
                        .with_position(Vec3::new(0.0, -0.05, 0.0))
                        .with_size(Vec3::new(8.0, 0.1, 8.0)),
                );
                engine.add_child(
                    Part::new()
                        .with_position(Vec3::new(0.0, 0.6, 0.0))
                        .with_size(Vec3::splat(1.2)),
                );
                let mut light_anchor = BasePart::new().with_position(Vec3::new(0.0, 2.5, 1.0));
                light_anchor.add_child(
                    PointLight::new()
                        .with_range(10.0)
                        .with_brightness(40.0)
                        .with_shadows(shadows),
                );
                engine.add_child(light_anchor);
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    let unshadowed = render(false)?;
    let shadowed = render(true)?;
    let changed_pixels = unshadowed
        .iter()
        .zip(&shadowed)
        .filter(|(unshadowed, shadowed)| {
            unshadowed[..3]
                .iter()
                .zip(&shadowed[..3])
                .any(|(unshadowed, shadowed)| unshadowed.abs_diff(*shadowed) > 2)
        })
        .count();

    assert!(
        changed_pixels > 0,
        "enabling point-light shadows should change rendered receiver occlusion"
    );
    Ok(())
}
