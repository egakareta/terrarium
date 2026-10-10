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
            let mesh = engine.add_mesh(include_bytes!("../assets/DamagedHelmet.glb"))?;
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
fn builtin_texture_loads_and_renders() -> Result<(), AppCreationError> {
    let _gpu_test_guard = serialize_gpu_test();
    let engine = Terrarium::new()
        .with_size([128, 128])
        .with_headless(Some(1))
        .run(|engine| {
            engine.lighting.clear_skybox();
            engine.with_clear_color([0.0, 0.0, 0.0, 1.0]);
            let wood = engine
                .workspace
                .load_builtin_texture(BuiltinTexture::Wood)?;
            engine.add_child(
                Part::new()
                    .with_position(Vec3::new(0.0, 1.0, 0.0))
                    .with_size(Vec3::splat(2.0))
                    .with_material(wood),
            );
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();

    let pixel = engine.renderer().read_pixel(64, 64)?;
    assert_ne!(pixel, [0, 0, 0, 255]);
    Ok(())
}
