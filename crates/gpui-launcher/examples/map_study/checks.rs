// SPDX-License-Identifier: MPL-2.0
use super::{Camera, Coverage, Selection};

#[test]
fn browsing_a_detached_window_preserves_the_recorded_visit() {
    let original = Selection::last(0);
    let mut detached = original;
    detached.instance = 1;
    assert_ne!(original.title(), detached.title());
    assert!(original.coverage() == Coverage::Saved);
    assert!(detached.coverage() == Coverage::Partial);
    detached.instance = 2;
    assert!(detached.coverage() == Coverage::Empty);
    assert!(original == Selection::last(0));
}

#[test]
fn camera_bounds_hold_across_successive_zoom_and_pan_inputs() {
    let mut camera = Camera::default();
    camera.magnify(40.);
    assert_eq!(camera.zoom, 8.);
    camera.pan(-999., 999.);
    assert_eq!((camera.x, camera.z), (0., 512.));
    camera.magnify(0.001);
    assert_eq!(camera.zoom, 0.5);
}
