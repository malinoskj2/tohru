use super::*;

fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
    Rect::new(x, y, w, h).unwrap()
}

#[test]
fn fractional_capture_rounds_crop_outward_on_the_actual_pixel_grid() {
    let output = Layout {
        logical: rect(-100, 0, 100, 80),
        pixels: (125, 100),
    };
    let result = plan(rect(-99, 1, 3, 2), &[output]).unwrap();
    assert_eq!(result.pixels, (4, 3));
    assert_eq!(result.pieces[0].source, rect(1, 1, 4, 3));
    assert_eq!(result.pieces[0].destination, rect(0, 0, 4, 3));
}

#[test]
fn mixed_scale_and_negative_origin_keep_cross_output_placement() {
    let outputs = [
        Layout {
            logical: rect(-100, 0, 100, 80),
            pixels: (100, 80),
        },
        Layout {
            logical: rect(0, 0, 100, 80),
            pixels: (200, 160),
        },
    ];
    let result = plan(rect(-10, 5, 20, 10), &outputs).unwrap();
    assert_eq!(result.pixels, (40, 20));
    assert_eq!(result.pieces[0].source, rect(90, 5, 10, 10));
    assert_eq!(result.pieces[0].destination, rect(0, 0, 20, 20));
    assert_eq!(result.pieces[1].source, rect(0, 10, 20, 20));
    assert_eq!(result.pieces[1].destination, rect(20, 0, 20, 20));
}

#[test]
fn rotation_is_already_upright_and_gaps_are_not_stretched_away() {
    let outputs = [
        Layout {
            logical: rect(0, 0, 30, 60),
            pixels: (60, 120),
        },
        Layout {
            logical: rect(50, 0, 30, 60),
            pixels: (60, 120),
        },
    ];
    let result = plan(rect(20, 10, 40, 20), &outputs).unwrap();
    assert_eq!(result.pixels, (80, 40));
    assert_eq!(result.pieces[0].destination, rect(0, 0, 20, 40));
    assert_eq!(result.pieces[1].destination, rect(60, 0, 20, 40));
}

#[test]
fn geometry_rejects_overflow_unknown_outputs_and_pixel_budget() {
    assert!(Rect::new(i32::MAX, 0, 1, 1).is_err());
    assert!(Rect::new(0, 0, 0, 1).is_err());
    assert!(plan(rect(0, 0, 10, 10), &[]).is_err());
    let output = Layout {
        logical: rect(0, 0, 10, 10),
        pixels: (20, 20),
    };
    assert!(matches!(
        plan(rect(100, 0, 10, 10), &[output]),
        Err(Error::Outside)
    ));
    let huge = Layout {
        logical: rect(0, 0, 100, 100),
        pixels: (8192, 8192),
    };
    assert!(plan(rect(0, 0, 10, 10), &[huge]).is_err());
    let repeated = [Layout {
        logical: rect(0, 0, 1, 1),
        pixels: (1, 1),
    }; 33];
    assert!(matches!(
        plan(rect(0, 0, 1, 1), &repeated),
        Err(Error::Outputs)
    ));
}

#[test]
fn below_one_density_keeps_actual_capture_pixels() {
    let output = Layout {
        logical: rect(0, 0, 80, 40),
        pixels: (60, 30),
    };
    assert_eq!(
        plan(rect(0, 0, 80, 40), &[output]).unwrap().pixels,
        (60, 30)
    );
}
