use super::*;

#[test]
fn input_growth_is_bounded_by_actual_read_not_just_prior_metadata() {
    assert!(matches!(
        read_limited(Cursor::new([0; 17]), 16),
        Err(Error::Bounds)
    ));
    assert_eq!(read_limited(Cursor::new([7; 16]), 16).unwrap(), [7; 16]);
}
#[test]
fn source_geometry_limits_precede_raster_allocation() {
    for size in [(0, 1), (1, 0), (MAX_SIDE + 1, 1), (8192, 8192)] {
        assert!(matches!(checked(size), Err(Error::Bounds)));
    }
    assert_eq!(checked((8192, 4096)).unwrap(), (8192, 4096));
    let selected = limits();
    assert_eq!(selected.max_image_width, Some(16384));
    assert_eq!(selected.max_alloc, Some(256 * 1024 * 1024));
}
#[test]
fn malformed_bytes_fail_without_path_reopen_or_unbounded_decode() {
    assert!(decode(b"invalid raster input").is_err());
}
