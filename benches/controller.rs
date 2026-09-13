#![feature(test)]

extern crate test;

use terrarium::{Camera, CameraController};
use test::{Bencher, black_box};

#[bench]
fn camera_controller_update(bencher: &mut Bencher) {
    let mut camera = Camera::default();
    let mut controller = CameraController::new(6.0, 0.0025);

    bencher.iter(|| {
        black_box(&mut controller).update_camera(black_box(&mut camera), black_box(1.0 / 60.0));
        black_box(camera.pv.pivot());
    });
}
