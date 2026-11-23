//tasks related to interfacing with the rotary encoder/ button
use modulino::Knob;

use crate::bus_init::TwiDevice;

#[embassy_executor::task]
pub async fn button_runner(i2c_p: TwiDevice) {
    let mut knob = Knob::new(i2c_p).unwrap();

    // Set a range for the encoder value
    // knob.set_range(-100, 100);

    // loop {
    //     if knob.update().expect("Bad coms with rotary encoder") {
    //         defmt::info!("Value: {}, Pressed: {}", knob.value(), knob.pressed());
    //     }
    // }
}
