//as described in lib.rs, basically this is here to generate a lookup table and optimize the pid parameters
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PidConfig {
    kp: f32,
    ki: f32,
    kd: f32,
}
