//Looking for this to be a relatively portable piece of code that allows for easy
// spi bus generation and defines some types for spi device calls

use core::cell::RefCell;

use embassy_rp::{
    bind_interrupts,
    gpio::AnyPin,
    i2c::{self, I2c, SclPin, SdaPin},
    interrupt::typelevel::I2C0_IRQ,
    peripherals::{DMA_CH0, DMA_CH1, I2C1, SPI1},
    spi::{self, Async, ClkPin, Instance, MisoPin, Mode, MosiPin, Spi},
    Peri,
};
use static_cell::StaticCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

//As soon as more spi devices are in use, this needs to change to async spibus
type SpiBus = embassy_sync::mutex::Mutex<CriticalSectionRawMutex, Spi<'static, SPI1, Async>>;

pub fn spi_init<Mosi, Miso, Clk>(
    spi_p: Peri<'static, SPI1>,
    mosi: Peri<'static, Mosi>,
    miso: Peri<'static, Miso>,
    clk: Peri<'static, Clk>,
    dma_tx: Peri<'static, DMA_CH1>,
    dma_rx: Peri<'static, DMA_CH0>,
) -> &'static SpiBus
where
    Mosi: MosiPin<SPI1>,
    Miso: MisoPin<SPI1>,
    Clk: ClkPin<SPI1>,
{
    bind_interrupts!(struct Irqs {
        DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH1>,embassy_rp::dma::InterruptHandler<DMA_CH0>;
    });

    let config = spi::Config::default();

    let spi_bus_raw = Spi::new(spi_p, clk, mosi, miso, dma_tx, dma_rx, Irqs, config);

    //wrap the bus in a static_cell so it lives on
    static SPI2_BUS: StaticCell<SpiBus> = StaticCell::new();
    SPI2_BUS.init(embassy_sync::mutex::Mutex::new(spi_bus_raw))
}

type TwiBus =
    embassy_sync::mutex::Mutex<CriticalSectionRawMutex, I2c<'static, I2C1, embassy_rp::i2c::Async>>;

pub fn i2c_init<Sda, Scl>(
    i2c: Peri<'static, I2C1>,
    sda: Peri<'static, Sda>,
    scl: Peri<'static, Scl>,
) -> &'static TwiBus
where
    Sda: SdaPin<I2C1>,
    Scl: SclPin<I2C1>,
{
    bind_interrupts!(struct Irqs {
        I2C1_IRQ => i2c::InterruptHandler<I2C1>;
    });

    //Configure the i2c bus
    let config = i2c::Config::default();

    //create the i2c bus
    let twi_bus = i2c::I2c::new_async(i2c, scl, sda, Irqs, config);

    //Wrap the i2c bus in mutex stuff so that the bus can be shared + return
    static TWI_BUS0: StaticCell<TwiBus> = StaticCell::new();
    TWI_BUS0.init(embassy_sync::mutex::Mutex::new(twi_bus))
}
