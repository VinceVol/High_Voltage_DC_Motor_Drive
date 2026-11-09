use num_traits::{AsPrimitive, PrimInt};

pub fn interpolate<T, W, const N: usize>(x_arr: &[T; N], y_arr: &[T; N], x_val: T) -> T
where
    T: PrimInt + Copy + 'static,
    W: PrimInt + AsPrimitive<T> + 'static,
    T: AsPrimitive<W>,
    W: From<T>,
{
    if x_val <= x_arr[0] {
        return y_arr[0];
    }
    if x_val >= x_arr[N - 1] {
        return y_arr[N - 1];
    }

    let idx = x_arr.partition_point(|&x| x <= x_val);
    let i = if idx == 0 { 0 } else { idx - 1 };
    let i = if i >= N - 1 { N - 2 } else { i };

    let x0: W = x_arr[i].as_();
    let x1: W = x_arr[i + 1].as_();
    let y0: W = y_arr[i].as_();
    let y1: W = y_arr[i + 1].as_();
    let x_w: W = x_val.as_();

    let dx = x1 - x0;
    if dx == W::zero() {
        return y0.as_();
    }

    let dy = y1 - y0;
    let result = y0 + ((x_w - x0) * dy) / dx;

    result.as_()
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_points() {
        let x: [u16; 5] = [0, 10, 20, 30, 40];
        let y: [u16; 5] = [100, 200, 300, 400, 500];

        // Testing exact matches on array points
        assert_eq!(interpolate::<u16, i32, 5>(&x, &y, 0), 100);
        assert_eq!(interpolate::<u16, i32, 5>(&x, &y, 10), 200);
        assert_eq!(interpolate::<u16, i32, 5>(&x, &y, 30), 400);
        assert_eq!(interpolate::<u16, i32, 5>(&x, &y, 40), 500);
    }

    #[test]
    fn test_midpoint_interpolation() {
        let x: [u16; 5] = [0, 10, 20, 30, 40];
        let y: [u16; 5] = [100, 200, 300, 400, 500];

        // Midpoint between 10 (200) and 20 (300) should be 250
        assert_eq!(interpolate::<u16, i32, 5>(&x, &y, 15), 250);

        // Quarter point: halfway between 0 (100) and 10 (200) -> 5 should give 150
        assert_eq!(interpolate::<u16, i32, 5>(&x, &y, 5), 150);
    }

    #[test]
    fn test_out_of_bounds_clamping() {
        let x: [u16; 4] = [10, 20, 30, 40];
        let y: [u16; 4] = [100, 200, 300, 400];

        // Below minimum x should clamp to first element
        assert_eq!(interpolate::<u16, i32, 4>(&x, &y, 5), 100);
        assert_eq!(interpolate::<u16, i32, 4>(&x, &y, 0), 100);

        // Above maximum x should clamp to last element
        assert_eq!(interpolate::<u16, i32, 4>(&x, &y, 45), 400);
        assert_eq!(interpolate::<u16, i32, 4>(&x, &y, 100), 400);
    }

    #[test]
    fn test_duplicate_x_values() {
        let x: [u16; 4] = [10, 20, 20, 40]; // Step change / duplicate x
        let y: [u16; 4] = [100, 200, 300, 400];

        // Should safely handle dx == 0 without crashing (returning y0)
        let val = interpolate::<u16, i32, 4>(&x, &y, 20);
        assert!(val == 200 || val == 300);
    }

    #[test]
    fn test_different_types_and_sizes() {
        // Testing with u8 and a different array size (N = 3)
        let x_u8: [u8; 3] = [5, 15, 25];
        let y_u8: [u8; 3] = [50, 100, 150];

        assert_eq!(interpolate::<u8, i16, 3>(&x_u8, &y_u8, 10), 75);

        // Testing with i16 (signed values)
        let x_i16: [i16; 3] = [-10, 0, 10];
        let y_i16: [i16; 3] = [-100, 0, 100];

        assert_eq!(interpolate::<i16, i32, 3>(&x_i16, &y_i16, -5), -50);
        assert_eq!(interpolate::<i16, i32, 3>(&x_i16, &y_i16, 5), 50);
    }

    #[test]
    fn test_single_element_array() {
        // N = 1 edge case (though rare for interpolation, should handle bounds safely)
        let x: [u16; 1] = [42];
        let y: [u16; 1] = [999];

        assert_eq!(interpolate::<u16, i32, 1>(&x, &y, 42), 999);
        assert_eq!(interpolate::<u16, i32, 1>(&x, &y, 10), 999);
        assert_eq!(interpolate::<u16, i32, 1>(&x, &y, 100), 999);
    }
}
