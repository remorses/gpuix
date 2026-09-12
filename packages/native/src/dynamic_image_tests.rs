use super::*;
use gpui::{RenderImage, Window};

const MAIN_THREAD_TEST: &str = "GPUIX_DYNAMIC_IMAGE_TEST";

// libtest spawns a worker even with --test-threads=1, but AppKit requires
// NSWindow creation on main. Re-exec only these GPU tests and enter their
// bodies before the child harness starts, without changing production code.
#[ctor::ctor]
fn main_thread_image_test() {
    let Ok(name) = std::env::var(MAIN_THREAD_TEST) else {
        return;
    };
    let result = std::panic::catch_unwind(|| match name.as_str() {
        "failure" => dynamic_image_failed_upload_is_not_retried_and_does_not_block_other_images(),
        "lifecycle" => dynamic_image_gpu_pixels_atlas_and_lifecycle(),
        _ => panic!("unknown dynamic image test: {name}"),
    });
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

fn run_on_main_thread(name: &str) -> bool {
    if std::env::var(MAIN_THREAD_TEST).as_deref() == Ok(name) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .env(MAIN_THREAD_TEST, name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GPU test {name}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    true
}

fn with_window<R>(f: impl FnOnce(&mut Window) -> R) -> R {
    with_test_state(|cx, window, _| {
        cx.update_window(window, |_, window, _| f(window))
            .map_err(|error| Error::from_reason(error.to_string()))
    })
    .unwrap()
}

fn has_atlas_entry(image: &Arc<RenderImage>) -> bool {
    with_window(|window| {
        // PlatformAtlas::contains defaults to false on Metal at this GPUI pin.
        // update_image reports real tile retention. Undo an absent-tile probe
        // immediately so it cannot repopulate an entry that cleanup removed.
        let retained = window.update_image(image.clone()).unwrap();
        if !retained {
            window.drop_image(image.clone()).unwrap();
        }
        retained
    })
}

fn staged(renderer: &TestGpuixRenderer) -> Arc<RenderImage> {
    renderer.tree.lock().unwrap().images[&2].clone()
}

fn upload(renderer: &TestGpuixRenderer, width: u32, height: u32, pixel: [u8; 4]) {
    renderer
        .update_image(
            2.0,
            width as f64,
            height as f64,
            Uint8Array::new(pixel.repeat((width * height) as usize)),
        )
        .unwrap();
}

fn assert_pixel(renderer: &TestGpuixRenderer, x: f32, y: f32, expected: [u8; 4]) {
    renderer.flush().unwrap();
    let actual = with_test_state(|cx, window, _| {
        let scale = cx
            .update_window(window, |_, window, _| window.scale_factor())
            .unwrap();
        let image = cx.capture_screenshot(window).unwrap();
        Ok(image.get_pixel((x * scale) as u32, (y * scale) as u32).0)
    })
    .unwrap();
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "pixel ({x}, {y}): {actual} != {expected}"
        );
    }
}

#[test]
fn dynamic_image_failed_upload_is_not_retried_and_does_not_block_other_images() {
    if run_on_main_thread("failure") {
        return;
    }
    use crate::dynamic_image::{self, Images};

    let _renderer = TestGpuixRenderer::new(Some(64.0), Some(64.0)).unwrap();
    let mut tree = RetainedTree::new();
    tree.create_element(1, "img".into());
    tree.create_element(2, "img".into());
    for id in [1.0, 2.0] {
        dynamic_image::update(&mut tree, id, 4.0, 4.0, &[255; 64]).unwrap();
    }
    let mut uploaded = Images::new();
    let mut failed = Images::new();
    with_window(|window| dynamic_image::sync(&tree.images, &mut uploaded, &mut failed, window));
    let first = uploaded[&1].clone();
    for id in [1.0, 2.0] {
        dynamic_image::update(&mut tree, id, 4.0, 4.0, &[0; 64]).unwrap();
    }
    let mut attempts = 0;
    for _ in 0..3 {
        with_window(|window| {
            dynamic_image::sync_with(
                &tree.images,
                &mut uploaded,
                &mut failed,
                window,
                |window, image| {
                    attempts += 1;
                    if image.id == first.id {
                        anyhow::bail!("injected allocation failure");
                    }
                    window.update_image(image)
                },
            );
        });
    }
    assert_eq!(attempts, 2);
    assert!(!uploaded.contains_key(&1));
    assert!(Arc::ptr_eq(&uploaded[&2], &tree.images[&2]));
    assert!(Arc::ptr_eq(&failed[&1], &tree.images[&1]));
    assert!(!has_atlas_entry(&first));

    dynamic_image::update(&mut tree, 1.0, 4.0, 4.0, &[255; 64]).unwrap();
    with_window(|window| dynamic_image::sync(&tree.images, &mut uploaded, &mut failed, window));
    assert!(failed.is_empty());
    assert!(Arc::ptr_eq(&uploaded[&1], &tree.images[&1]));
    assert!(has_atlas_entry(&first));

    dynamic_image::update(&mut tree, 1.0, 4.0, 4.0, &[0; 64]).unwrap();
    with_window(|window| {
        dynamic_image::sync_with(&tree.images, &mut uploaded, &mut failed, window, |_, _| {
            anyhow::bail!("injected allocation failure")
        });
    });
    let weak = Arc::downgrade(&failed[&1]);
    dynamic_image::clear(&mut tree, 1.0).unwrap();
    tree.destroy_element(2);
    with_window(|window| dynamic_image::sync(&tree.images, &mut uploaded, &mut failed, window));
    assert!(uploaded.is_empty());
    assert!(failed.is_empty());
    assert!(weak.upgrade().is_none());
}

#[test]
fn dynamic_image_gpu_pixels_atlas_and_lifecycle() {
    if run_on_main_thread("lifecycle") {
        return;
    }
    let renderer = TestGpuixRenderer::new(Some(64.0), Some(64.0)).unwrap();
    renderer
        .apply_batch(
            r##"[
        ["createElement",1,"div"],
        ["setStyle",1,{"width":64,"height":64,"backgroundColor":"#00ff00"}],
        ["createElement",2,"img"],
        ["setStyle",2,{"width":64,"height":64}],
        ["appendChild",1,2],["setRoot",1]
    ]"##
            .into(),
        )
        .unwrap();

    let top = [
        [0, 0, 255, 255],
        [0, 0, 255, 255],
        [255, 0, 0, 255],
        [255, 0, 0, 255],
    ]
    .concat();
    let bottom = [
        [0, 255, 0, 255],
        [0, 255, 0, 255],
        [255, 255, 255, 255],
        [255, 255, 255, 255],
    ]
    .concat();
    let pixels = [top.repeat(2), bottom.repeat(2)].concat();
    renderer
        .update_image(2.0, 4.0, 4.0, Uint8Array::new(pixels))
        .unwrap();
    assert_pixel(&renderer, 16.0, 16.0, [255, 0, 0, 255]);
    assert_pixel(&renderer, 48.0, 16.0, [0, 0, 255, 255]);
    assert_pixel(&renderer, 16.0, 48.0, [0, 255, 0, 255]);
    assert_pixel(&renderer, 48.0, 48.0, [255, 255, 255, 255]);

    upload(&renderer, 4, 4, [0, 0, 255, 255]);
    let first = staged(&renderer);
    renderer.flush().unwrap();
    assert!(has_atlas_entry(&first));
    assert_pixel(&renderer, 32.0, 32.0, [255, 0, 0, 255]);

    upload(&renderer, 4, 4, [255, 0, 0, 255]);
    let second = staged(&renderer);
    assert_eq!(second.id, first.id);
    assert!(!Arc::ptr_eq(&first, &second));
    // Assert GPUI's real atlas retention result, not just the host's stable id.
    assert!(with_window(|window| window
        .update_image(second.clone())
        .unwrap()));
    assert_pixel(&renderer, 32.0, 32.0, [0, 0, 255, 255]);

    upload(&renderer, 8, 4, [0, 0, 255, 128]);
    let resized = staged(&renderer);
    assert_ne!(resized.id, second.id);
    assert!(!has_atlas_entry(&resized));
    assert_pixel(&renderer, 32.0, 32.0, [128, 127, 0, 255]);
    assert!(!has_atlas_entry(&second));
    assert!(has_atlas_entry(&resized));
    // A wide bitmap is contained, leaving the parent's green fill above/below.
    assert_pixel(&renderer, 32.0, 4.0, [0, 255, 0, 255]);

    renderer.apply_batch(r#"[["setCustomProp",2,"objectFit","fill"],["setStyle",2,{"width":64,"height":64,"borderRadius":32}]]"#.into()).unwrap();
    upload(&renderer, 8, 4, [0, 0, 255, 255]);
    assert_pixel(&renderer, 32.0, 32.0, [255, 0, 0, 255]);
    assert_pixel(&renderer, 2.0, 2.0, [0, 255, 0, 255]);
    upload(&renderer, 8, 4, [255, 0, 255, 0]);
    assert_pixel(&renderer, 32.0, 32.0, [0, 255, 0, 255]);

    // A real src change takes over; retransmitting the same src does not.
    renderer
        .apply_batch(r#"[["setCustomProp",2,"src","https://example.test/image.svg"]]"#.into())
        .unwrap();
    renderer.flush().unwrap();
    assert!(renderer.tree.lock().unwrap().images.is_empty());
    assert!(!has_atlas_entry(&resized));
    upload(&renderer, 4, 4, [255, 0, 0, 255]);
    let overridden = staged(&renderer);
    renderer
        .apply_batch(r#"[["setCustomProp",2,"src","https://example.test/image.svg"]]"#.into())
        .unwrap();
    assert_eq!(staged(&renderer).id, overridden.id);
    assert_pixel(&renderer, 32.0, 32.0, [0, 0, 255, 255]);
    renderer.clear_image(2.0).unwrap();
    renderer.clear_image(2.0).unwrap();
    renderer.flush().unwrap();
    assert!(!has_atlas_entry(&overridden));
    assert!(renderer.tree.lock().unwrap().images.is_empty());

    // Clear + replace before painting must still retire the old atlas identity.
    upload(&renderer, 4, 4, [0, 0, 255, 255]);
    let before_clear = staged(&renderer);
    renderer.flush().unwrap();
    renderer.clear_image(2.0).unwrap();
    upload(&renderer, 4, 4, [255, 0, 0, 255]);
    let last = staged(&renderer);
    assert_ne!(last.id, before_clear.id);
    renderer.flush().unwrap();
    assert!(!has_atlas_entry(&before_clear));
    renderer
        .apply_batch(r#"[["destroyElement",1]]"#.into())
        .unwrap();
    renderer.flush().unwrap();
    assert!(!has_atlas_entry(&last));
    assert!(renderer.tree.lock().unwrap().images.is_empty());
    assert!(renderer.clear_image(2.0).is_err());
    drop((first, second, resized, overridden, before_clear));
    let weak = Arc::downgrade(&last);
    drop(last);
    assert!(weak.upgrade().is_none());
}
