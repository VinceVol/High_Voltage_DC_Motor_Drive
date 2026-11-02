// Here to send communications from mesa over channels for the main loop to react to

use defmt::info;
use embassy_rp::{gpio::Input, peripherals::PIN_0, Peri};

use crate::MESA_DIR_REQ_SIG;

#[embassy_executor::task]
pub async fn read_mesa_dir(dir_set: Peri<'static, PIN_0>) {
    let mut dir_in = Input::new(dir_set, embassy_rp::gpio::Pull::Up);
    loop {
        dir_in.wait_for_any_edge().await;
        info!("Saw dir edge!");
        match dir_in.get_level() {
            embassy_rp::gpio::Level::High => {
                MESA_DIR_REQ_SIG.signal(crate::direction::Direction::Forward)
            }
            embassy_rp::gpio::Level::Low => {
                MESA_DIR_REQ_SIG.signal(crate::direction::Direction::Reverse)
            }
        }
    }
}
