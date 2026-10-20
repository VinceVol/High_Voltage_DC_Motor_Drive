//Thinking this represents the motor struct and task runner overall. ~big brain ish portion of the firware that decides overall what the motor is doing
// I think this should include the PID bits but I haven't fully decided on the best way to structure this

use embassy_rp::{
    adc::{Adc, Channel, Config},
    bind_interrupts,
    peripherals::{ADC, PIN_0, PIN_1, PIN_27},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Receiver, Sender},
};

use crate::direction::Direction;

#[embassy_executor::task]
pub async fn run_motor(
    spindle_set: Peri<'static, PIN_27>, //pin 27
    dir_set: Peri<'static, PIN_0>,
    adc_p: Peri<'static, ADC>,
    en_drives: Peri<'static, PIN_1>,
    dir_sender: Sender<'static, CriticalSectionRawMutex, Direction, 25>,
    pwm_sender: Sender<'static, CriticalSectionRawMutex, u16, 25>,
) {
    bind_interrupts!(struct Irqs {
        ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
    });

    let mut spindle_cmd = Channel::new_pin(spindle_set, embassy_rp::gpio::Pull::None);

    let config = Config::default();
    let mut adc = Adc::new(adc_p, Irqs, config);
    let _ = adc.read(&mut spindle_cmd).await;
    let mut v_stpt: u16 = 0;
    loop {
        todo!();
    }
}
