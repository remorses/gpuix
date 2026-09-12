//! Direct pixel overrides for retained `<img>` nodes. No JS storage crosses a frame.
use std::{collections::HashMap, sync::Arc};

use gpui::{RenderImage, Window};

use crate::retained_tree::RetainedTree;

pub(crate) type Images = HashMap<u64, Arc<RenderImage>>;

// Bound ingress memory and stay below desktop atlas texture limits. This is a
// resource limit, not layout geometry or a promise that GPU allocation succeeds.
const MAX_IMAGE_DIMENSION: u32 = 4096;

fn image_element(tree: &RetainedTree, id: f64) -> Result<u64, String> {
    if !id.is_finite() || id < 0.0 || id.fract() != 0.0 || id > 9_007_199_254_740_991.0 {
        return Err("elementId must be a non-negative safe integer".into());
    }
    let id = id as u64;
    if !tree
        .elements
        .get(&id)
        .is_some_and(|el| el.element_type == "img")
    {
        return Err("elementId must identify an existing img in this renderer".into());
    }
    Ok(id)
}

pub(crate) fn update(
    tree: &mut RetainedTree,
    id: f64,
    width: f64,
    height: f64,
    bgra: &[u8],
) -> Result<(), String> {
    let id = image_element(tree, id)?;
    let dimension = |value: f64| {
        if !value.is_finite() || value < 1.0 || value.fract() != 0.0 || value > u32::MAX as f64 {
            Err("image dimensions must be positive 32-bit integers".to_string())
        } else {
            Ok(value as u32)
        }
    };
    let (width, height) = (dimension(width)?, dimension(height)?);
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("image byte length overflow")?;
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(format!(
            "image dimensions must not exceed {MAX_IMAGE_DIMENSION} pixels per axis"
        ));
    }
    if bgra.len() != len {
        return Err(format!(
            "BGRA byte length must be exactly {len}, got {}",
            bgra.len()
        ));
    }
    // from_bgra accepts an owned Vec. Copy the typed-array view now, before NAPI returns.
    let mut image =
        RenderImage::from_bgra(width, height, bgra.to_vec()).ok_or("invalid BGRA image")?;
    if let Some(previous) = tree
        .images
        .get(&id)
        .filter(|previous| previous.size(0) == image.size(0))
    {
        // Window::update_image explicitly uses RenderImage::id to preserve atlas identity.
        image.id = previous.id;
    }
    tree.images.insert(id, Arc::new(image));
    tree.mark_render_changed(id);
    Ok(())
}

pub(crate) fn clear(tree: &mut RetainedTree, id: f64) -> Result<(), String> {
    let id = image_element(tree, id)?;
    if tree.images.remove(&id).is_some() {
        tree.mark_render_changed(id);
    }
    Ok(())
}

/// Upload only changed images and evict overrides cleared or destroyed since the last frame.
/// All maps are renderer-owned; uploaded and failed versions live with their GPUI window.
pub(crate) fn sync(
    images: &Images,
    uploaded: &mut Images,
    failed: &mut Images,
    window: &mut Window,
) {
    sync_with(images, uploaded, failed, window, Window::update_image);
}

pub(crate) fn sync_with(
    images: &Images,
    uploaded: &mut Images,
    failed: &mut Images,
    window: &mut Window,
    mut upload: impl FnMut(&mut Window, Arc<RenderImage>) -> anyhow::Result<bool>,
) {
    failed.retain(|id, image| images.get(id).is_some_and(|next| Arc::ptr_eq(next, image)));
    uploaded.retain(|id, image| {
        let keep = images.get(id).is_some_and(|next| next.id == image.id);
        if !keep {
            // GPUI owns atlas reclamation; dropping the CPU Arc alone does not evict a tile.
            if let Err(error) = window.drop_image(image.clone()) {
                log::error!("Failed to drop dynamic image: {error}");
            }
        }
        keep
    });
    for (&id, image) in images {
        if uploaded.get(&id).is_some_and(|old| Arc::ptr_eq(old, image)) || failed.contains_key(&id)
        {
            continue;
        }
        if let Err(error) = upload(window, image.clone()) {
            log::error!("Failed to upload dynamic image: {error}");
            // GPUI may have removed the previous tile before an allocation failed.
            // drop_image safely removes any remaining entry. Do not pass this
            // version to Img, whose paint would otherwise retry the allocation.
            if let Err(error) = window.drop_image(image.clone()) {
                log::error!("Failed to drop dynamic image: {error}");
            }
            uploaded.remove(&id);
            failed.insert(id, image.clone());
            continue;
        }
        uploaded.insert(id, image.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> RetainedTree {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "img".into());
        tree
    }

    #[test]
    fn copies_pixels_preserves_same_size_identity_and_replaces_on_resize() {
        let mut tree = tree();
        let mut bytes = vec![3, 2, 1, 255];
        update(&mut tree, 1.0, 1.0, 1.0, &bytes).unwrap();
        let first = tree.images[&1].clone();
        bytes.fill(0);
        assert_eq!(first.as_bytes(0), Some([3, 2, 1, 255].as_slice()));
        update(&mut tree, 1.0, 1.0, 1.0, &[255; 4]).unwrap();
        assert_eq!(tree.images[&1].id, first.id);
        update(&mut tree, 1.0, 2.0, 1.0, &[255; 8]).unwrap();
        assert_ne!(tree.images[&1].id, first.id);
        assert_eq!(tree.images[&1].size(0), gpui::size(2.into(), 1.into()));
        assert!(!Arc::ptr_eq(&first, &tree.images[&1]));
        clear(&mut tree, 1.0).unwrap();
        clear(&mut tree, 1.0).unwrap();
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        assert_ne!(tree.images[&1].id, first.id);
    }

    #[test]
    fn rejects_invalid_input_without_replacing_pixels() {
        let mut tree = tree();
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        let first = tree.images[&1].clone();
        for dimension in [
            0.0,
            -1.0,
            1.5,
            f64::NAN,
            f64::INFINITY,
            u32::MAX as f64 + 1.0,
        ] {
            assert!(update(&mut tree, 1.0, dimension, 1.0, &[0; 4]).is_err());
            assert!(update(&mut tree, 1.0, 1.0, dimension, &[0; 4]).is_err());
        }
        for bytes in [vec![], vec![0; 3], vec![0; 5]] {
            assert!(update(&mut tree, 1.0, 1.0, 1.0, &bytes).is_err());
        }
        assert!(update(&mut tree, 1.0, u32::MAX as f64, u32::MAX as f64, &[]).is_err());
        for id in [
            -1.0,
            1.5,
            f64::NAN,
            f64::INFINITY,
            9_007_199_254_740_992.0,
            2.0,
        ] {
            assert!(update(&mut tree, id, 1.0, 1.0, &[0; 4]).is_err());
            assert!(clear(&mut tree, id).is_err());
        }
        assert!(Arc::ptr_eq(&first, &tree.images[&1]));
        tree.create_element(2, "div".into());
        assert!(update(&mut tree, 2.0, 1.0, 1.0, &[0; 4]).is_err());
    }

    #[test]
    fn rejects_oversized_dimensions_before_copying_or_length_validation() {
        let mut tree = tree();
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        let first = tree.images[&1].clone();
        for (width, height) in [(4097.0, 1.0), (1.0, 4097.0), (1_000_000.0, 1.0)] {
            assert!(update(&mut tree, 1.0, width, height, &[])
                .unwrap_err()
                .contains("4096"));
            assert!(Arc::ptr_eq(&first, &tree.images[&1]));
        }
        update(&mut tree, 1.0, 4096.0, 1.0, &vec![0; 4096 * 4]).unwrap();
        update(&mut tree, 1.0, 1.0, 4096.0, &vec![0; 4096 * 4]).unwrap();
    }

    #[test]
    fn actual_src_mutations_clear_overrides_but_identical_values_do_not() {
        let mut tree = tree();
        tree.set_custom_prop(1, "src".into(), "one.png".into());
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        tree.set_custom_prop(1, "src".into(), "one.png".into());
        assert_eq!(tree.images.len(), 1);
        tree.set_custom_prop(1, "src".into(), "two.png".into());
        assert!(tree.images.is_empty());
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        tree.set_custom_prop(1, "src".into(), serde_json::Value::Null);
        assert!(tree.images.is_empty());
    }

    #[test]
    fn destruction_and_id_reuse_release_staged_images() {
        let mut tree = tree();
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        let image = Arc::downgrade(&tree.images[&1]);
        tree.destroy_element(1);
        assert!(tree.images.is_empty());
        assert!(image.upgrade().is_none());
        tree.create_element(1, "img".into());
        update(&mut tree, 1.0, 1.0, 1.0, &[0; 4]).unwrap();
        tree.create_element(1, "div".into());
        assert!(tree.images.is_empty());
    }
}
