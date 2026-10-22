# hv-moto-coto
This folder contains all the rust code driving the rpi pico 2 w. A few things to note:
- *core_logic* is the folder that contains things that can be unit tested. This makes testing math related parts a bit easier
- *firmware* contains anything that is directly related to embassy_rp and/or the rpi pico target.
  - The logic on the pico is basically running on the embassy_rp crate and using channels to communicate information between different tasks

# Active Todos!
- [x] Create a PID loop that takes the requested rpm and outputs a pwm
- [ ] Create tasks that read the current and voltage and do something with it
- [x] Implement direction switching capabilities
