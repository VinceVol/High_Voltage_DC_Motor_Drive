//Looking for this to be a relatively portable piece of code that allows for easy
// spi bus generation and defines some types for spi device calls

use core::cell::RefCell;

use embassy_rp::{
    Peri, bind_interrupts,
    gpio::AnyPin,
    peripherals::SPI1,
    spi::{Async, Instance, Mode, Spi},
};
use static_cell::StaticCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

//As soon as more spi devices are in use, this needs to change to async spibus
type SpiBus = embassy_sync::mutex::Mutex<CriticalSectionRawMutex, Spi<'static, SPI1, Async>>;

pub fn spi_init(
    spi: Peri<'static, SPI1>,
    mosi: Peri<'static, AnyPin>,
    miso: Peri<'static, AnyPin>,
    clk: Peri<'static, AnyPin>,
) -> &'static SpiBus {
    bind_interrupts!(struct Irqs {
    SPI1 => InterruptHandler<SPI1>;
    });

    //set to a lower priority so softdevice ble works
    embassy_nrf::interrupt::SPI2.set_priority(Priority::P3);

    // 1. Configure the SPI Peripheral (The Bus)
    let mut config = spim::Config::default();
    config.frequency = spim::Frequency::M1; // 1 MHz

    // Create the SPIM (SPI Master with DMA)
    let spi_bus_raw = spim::Spim::new(spi, Irqs, clk, miso, mosi, config);

    //wrap the bus in a static_cell so it lives on
    static SPI2_BUS: StaticCell<SpiBus> = StaticCell::new();
    SPI2_BUS.init(embassy_sync::mutex::Mutex::new(spi_bus_raw))
}

type TwiBus = embassy_sync::mutex::Mutex<CriticalSectionRawMutex, Twim<'static>>;

pub fn i2c_init(
    i2c: Peri<'static, peripherals::TWISPI0>,
    sda: Peri<'static, AnyPin>,
    scl: Peri<'static, AnyPin>,
) -> &'static TwiBus {
    bind_interrupts!(struct Irqs {
        TWISPI0 => twim::InterruptHandler<peripherals::TWISPI0>;
    });

    //set to a lower priority so softdevice ble works
    embassy_nrf::interrupt::TWISPI0.set_priority(Priority::P3);

    //Generate a staticcell tx_buf that lives on
    static TX_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    let tx_buf = TX_BUF.init([0u8; 128]);

    //Configure the i2c bus
    let config = twim::Config::default();

    //create the i2c bus
    let twi_bus = twim::Twim::new(i2c, Irqs, sda, scl, config, tx_buf);

    //Wrap the i2c bus in mutex stuff so that the bus can be shared + return
    static TWI_BUS0: StaticCell<TwiBus> = StaticCell::new();
    TWI_BUS0.init(embassy_sync::mutex::Mutex::new(twi_bus))
}
