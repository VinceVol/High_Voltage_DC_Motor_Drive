//This is here to keep track of how the controller is doing and basically determining whether the
// gate driver should be enabled and whether current/voltage is too high

#[derive(Clone, Copy)]
pub enum ControllerCond {
    Ready,
    OverCurrent,
    OverVoltage,
    Estop,
}
