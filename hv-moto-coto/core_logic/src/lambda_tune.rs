//using this to convert the time to 1 time constant information we can get from the autotune setup
// into a P and a I term that we can save onto the flash

use crate::interpolation::interpolate_flt;

pub fn find_pi(
    multiplier: f32,
    k: f32, //setpnt change / control input change (rpm/pwm)
    times: [f32; 2],
    currents: [f32; 2],
    time_const_cur: f32,
) -> (f32, f32) {
    //interpolate the time it took to get to 0.6321 the RPM we wanted to get thou
    let thou = interpolate_flt::<f32, 2>(&currents, &times, time_const_cur);

    //Lambda is our one custom parameter that sets the general response speed
    let lambda = multiplier * thou;

    //With the two above we can get our PI parameters
    let kp = thou / (k * lambda);
    let ki = kp / thou;

    return (kp, ki);
}
