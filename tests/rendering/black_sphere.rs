async fn black_sphere_is_rendered_in_the_center_test() {
    let pixels = render_workspace_pixels(|workspace| {
        workspace.add_child(
            Part::new()
                .with_shape(PartShape::Ball)
                .with_color(terrarium::Color3::new(0.0, 0.0, 0.0))
                .with_size(Vec3::splat(2.0)),
        );
    })
    .await;

    let center = pixel(&pixels, WIDTH / 2, HEIGHT / 2);
    assert!(
        center[..3].iter().all(|channel| *channel < 16),
        "expected a black sphere at the screen center, got {center:?}"
    );
    assert_eq!(pixel(&pixels, 0, 0), WHITE);

    let dark_pixel_count = pixels
        .iter()
        .filter(|pixel| pixel[..3].iter().all(|channel| *channel < 16))
        .count();
    assert!(
        dark_pixel_count > 100,
        "expected a visible sphere, found only {dark_pixel_count} dark pixels"
    );
}
