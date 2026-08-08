use nara_ui_kernel::{
    UiPoint, UiRect, UiSafeAreaInsets, UiSize, UiSnapPolicy, UiSurfaceContentTransform,
    UiSurfaceContentTransformError,
};

#[test]
fn uniform_fit_centers_letterboxed_content_and_round_trips_platform_input() {
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(960.0, 540.0).unwrap(),
        UiSize::new(1024.0, 768.0).unwrap(),
        UiSafeAreaInsets::default(),
        1.25,
    )
    .unwrap();

    assert!(transform.physical_content_origin().x().abs() < 0.000_1);
    assert!((transform.physical_content_origin().y() - 96.0).abs() < 0.000_1);
    assert!((transform.physical_content_size().width() - 1024.0).abs() < 0.000_1);
    assert!((transform.physical_content_size().height() - 576.0).abs() < 0.000_1);
    assert!((transform.content_scale_factor() - 1024.0 / 960.0).abs() < 0.000_1);

    let platform_center = UiPoint::new(512.0 / 1.25, 384.0 / 1.25);
    let authored_center = transform
        .platform_logical_to_authored(platform_center)
        .unwrap();
    assert!((authored_center.x() - 480.0).abs() < 0.000_1);
    assert!((authored_center.y() - 270.0).abs() < 0.000_1);
    assert!(
        transform
            .platform_logical_to_authored(UiPoint::new(100.0, 10.0))
            .is_none()
    );
}

#[test]
fn authored_forward_mapping_round_trips_through_safe_area_and_fractional_platform_scale() {
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(100.0, 50.0).unwrap(),
        UiSize::new(400.0, 300.0).unwrap(),
        nara_ui_kernel::UiSafeAreaInsets::new(10.0, 20.0, 30.0, 40.0).unwrap(),
        1.25,
    )
    .unwrap();
    let authored = nara_ui_kernel::UiPoint::new(10.0, 20.0);
    let physical = transform
        .authored_to_physical(authored)
        .expect("an interior authored point should map to physical space");
    assert!((physical.x() - 46.0).abs() < 1.0e-5);
    assert!((physical.y() - 122.0).abs() < 1.0e-5);
    let platform = nara_ui_kernel::UiPoint::new(
        physical.x() / transform.platform_scale_factor(),
        physical.y() / transform.platform_scale_factor(),
    );
    let round_trip = transform
        .platform_logical_to_authored(platform)
        .expect("the mapped physical point should remain inside content");
    assert!((round_trip.x() - authored.x()).abs() < 1.0e-5);
    assert!((round_trip.y() - authored.y()).abs() < 1.0e-5);
    assert!(
        transform
            .authored_to_physical(nara_ui_kernel::UiPoint::new(100.0, 20.0))
            .is_none()
    );
    assert!(
        transform
            .physical_to_authored(nara_ui_kernel::UiPoint::new(10.0, 20.0))
            .is_none()
    );
    assert!(
        transform
            .physical_to_authored(nara_ui_kernel::UiPoint::new(10.0, 50.0))
            .is_some()
    );
}

#[test]
fn safe_area_is_removed_before_centered_uniform_fit() {
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(960.0, 540.0).unwrap(),
        UiSize::new(1200.0, 800.0).unwrap(),
        UiSafeAreaInsets::new(100.0, 50.0, 100.0, 50.0).unwrap(),
        1.0,
    )
    .unwrap();

    assert!((transform.physical_content_origin().x() - 100.0).abs() < 0.000_1);
    assert!((transform.physical_content_origin().y() - 118.75).abs() < 0.000_1);
    assert!((transform.physical_content_size().width() - 1000.0).abs() < 0.000_1);
    assert!((transform.physical_content_size().height() - 562.5).abs() < 0.000_1);
}

#[test]
fn mapped_rects_are_clipped_to_authored_content_before_snapping() {
    let transform =
        UiSurfaceContentTransform::direct(UiSize::new(100.0, 50.0).unwrap(), 2.0).unwrap();
    let clipped = transform
        .authored_rect_to_physical(
            UiRect::from_origin_size(-10.0, -5.0, 20.0, 15.0).unwrap(),
            UiSnapPolicy::NearestEdge,
        )
        .unwrap();

    assert_eq!(clipped.min_x(), 0);
    assert_eq!(clipped.min_y(), 0);
    assert_eq!(clipped.max_x(), 20);
    assert_eq!(clipped.max_y(), 20);
    assert!(
        transform
            .authored_rect_to_physical(
                UiRect::from_origin_size(120.0, 60.0, 10.0, 10.0).unwrap(),
                UiSnapPolicy::NearestEdge,
            )
            .is_none()
    );
}

#[test]
fn transform_rejects_consumed_safe_area_and_unbounded_scale() {
    assert!(matches!(
        UiSurfaceContentTransform::new(
            UiSize::new(100.0, 100.0).unwrap(),
            UiSize::new(200.0, 100.0).unwrap(),
            UiSafeAreaInsets::new(100.0, 0.0, 100.0, 0.0).unwrap(),
            1.0,
        ),
        Err(UiSurfaceContentTransformError::SafeAreaConsumesSurface { .. })
    ));
    assert!(matches!(
        UiSurfaceContentTransform::direct(UiSize::new(100.0, 100.0).unwrap(), 9.0),
        Err(UiSurfaceContentTransformError::InvalidContentScaleFactor { .. })
    ));
}
