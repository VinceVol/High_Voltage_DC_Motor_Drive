// Here to send communications from mesa over channels for the main loop to react to

use defmt::info;
use embassy_futures::select::{select, Either};
use embassy_rp::{gpio::Input, peripherals::PIN_0, Peri};

use crate::{AUTOTUNE_OVERRIDE, DIR_REQ_SIG};

#[embassy_executor::task]
pub async fn read_mesa_dir(dir_set: Peri<'static, PIN_0>) {
    let mut dir_in = Input::new(dir_set, embassy_rp::gpio::Pull::Up);

    //need to keep track of autotune to ignore the mesa inputs
    let mut autotune_enabled: bool = false;
    let mut autotune_watch = AUTOTUNE_OVERRIDE.receiver().unwrap();
    loop {
        //If new autotune setting arrives or mesa input changes then act on it
        match select(dir_in.wait_for_any_edge(), autotune_watch.changed()).await {
            Either::First(_) => {
                if autotune_enabled {
                    continue;
                };
                match dir_in.get_level() {
                    embassy_rp::gpio::Level::High => {
                        DIR_REQ_SIG.signal(crate::direction::Direction::Forward)
                    }
                    embassy_rp::gpio::Level::Low => {
                        DIR_REQ_SIG.signal(crate::direction::Direction::Reverse)
                    }
                }
            }
            Either::Second(new_tune) => {
                autotune_enabled = new_tune;
            }
        }
    }
}
