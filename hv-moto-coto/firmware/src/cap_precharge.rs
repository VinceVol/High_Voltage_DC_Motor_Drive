//Dont wanna blow everything up so on startup the big ol boy caps go through a resistor first
// watch the cap at the voltage and wait till they reach some threshold before closing Q5

use defmt::info;
use embassy_rp::{gpio::Output, peripherals::PIN_20, Peri};

use crate::{CAP_PRE_STATE_SIG, V_PRECH_THRESH, V_WATCH};

pub enum PrechargeState {
    StartUp,
    NormalOp,
}

#[embassy_executor::task]
pub async fn run_precharge() {
    //Q5 is the mosfet that enables bypassing the precharge resistor
    let mut v_watch = V_WATCH.receiver().unwrap();
    let mut v;

    //Let the motor loop know we're precharging the caps
    CAP_PRE_STATE_SIG.signal(PrechargeState::StartUp);

    loop {
        v = v_watch.changed().await;
        info!("The current precharge voltage = {}", v);

        if v >= V_PRECH_THRESH {
            //tell the rest of the system to proceed
            CAP_PRE_STATE_SIG.signal(PrechargeState::NormalOp);

            //TODO is there any scenario where it's worth turning q5 back on? TODO
            info!("Finished precharging the caps!");
            break;
        }
    }
}
