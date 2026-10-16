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
fn directional_shadow_filter_softness_changes_shadow_edges() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    fn render(softness: f32) -> Result<Vec<[u8; 4]>, AppCreationError> {
        let engine = Terrarium::new()
            .with_size([128, 128])
            .with_headless(Some(1))
            .run(move |engine| {
                engine.workspace.current_camera =
                    Camera::new(Vec3::new(4.0, 4.0, 6.0), Vec3::new(0.0, 0.0, 0.0), 1.0);
                engine
                    .lighting
                    .with_sun_direction(Vec3::new(-0.4, 1.0, 0.3));
                engine.lighting.shadow_softness = softness;
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
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();

        Ok(engine.renderer().read_pixels()?)
    }

    let hard = render(0.0)?;
    let soft = render(1.0)?;
    let changed_pixels = hard
        .iter()
        .zip(&soft)
        .filter(|(hard, soft)| {
            hard[..3]
                .iter()
                .zip(&soft[..3])
                .any(|(hard, soft)| hard.abs_diff(*soft) > 2)
        })
        .count();

    assert!(
        changed_pixels > 0,
        "directional shadow softness should change rendered shadow filtering"
    );
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
