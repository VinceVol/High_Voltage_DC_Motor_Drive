//using this to convert the time to 1 time constant information we can get from the autotune setup
// into a P and a I term that we can save onto the flash

use crate::interpolation::interpolate;

pub fn find_pi(
    multiplier: f32,
    k: f32, //setpnt change / control input change (rpm/pwm)
    times: [u64; 2],
    rpms: [u64; 2],
    time_const_rpm: u64,
) -> (f32, f32) {
    //interpolate the time it took to get to 0.6321 the RPM we wanted to get thou
    let thou = interpolate::<u64, i128, 2>(&rpms, &times, time_const_rpm) as f32;

    //Lambda is our one custom parameter that sets the general response speed
    let lambda = multiplier * thou;

    //With the two above we can get our PI parameters
    let kp = thou / (k * lambda);
    let ki = kp / thou;

    return (kp, ki);
}
