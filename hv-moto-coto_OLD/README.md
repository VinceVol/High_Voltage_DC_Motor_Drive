# HIGH VOLTAGE DC Motor Controller Software

The goals for this project are somewhat simple:
- Closed loop motor speed control, whilst monitoring other things
    - Current Draw doesn't exceed certain amounts
    - User Inputs
    - E stop
- Various methods of telling the mcu what speed to drive the motor at
    - Voltage
    - spi
    - uarte
    - i2c

## Todos

These are rough todos still given none of this has been started

-[ ] Communicate with MCU
-[ ] Turn on and off the mosfet gates through low voltage
-[ ] Create a PID with Speed Sensor

