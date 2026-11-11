# hv-moto-coto
This folder contains all the rust code driving the rpi pico 2 w. A few things to note:
- *core_logic* is the folder that contains things that can be unit tested. This makes testing math related parts a bit easier
- *firmware* contains anything that is directly related to embassy_rp and/or the rpi pico target.
  - The logic on the pico is basically running on the embassy_rp crate and using channels to communicate information between different tasks

# Active Todos!
- [x] Create a PID loop that takes the requested rpm and outputs a pwm
- [x] Create tasks that read the current and voltage and do something with it
- [x] Implement direction switching capabilities
- [x] Create a precharge setup for the main caps
- [ ] Figure out what's blocking pwm signal on direction changes
- [ ] Develope autotune script that saves settings to flash

# firmware
## Autotune
Despite the advice of my friends, I find the ability to plug any DC motor into this controller and the ability to tune it via the press of a button
quite appealing. The method I'm using to tune the P & I of the PID loop is called **Lambda Tuning**.

<div align="center">
  <img src="../images/Lambda Tuning Equations.png" alt="Lambda Equations" width="300">
</div>

<p align="center">
  <b>Figure 1:</b> <i>Lambda Tuning Equations</i>
</p>

The plan is to take the pwm from *pwma = xxx* and drive it to *pwmab == yyy*. The steady state rpm should also be recorded at *rpma* and *rpmb*. Then, log all the time to reach various rpms and
interpolate the rpm readout of *t = rpma + 0.632(rpmb-rpma)*. The *0.632* represents 1 time constant in the equations below.

<div align="center">
  <img src="../images/Time Constant Equtions.png" alt="Time Constant Eq" width="100">
</div>

<p align="center">
  <b>Figure 2:</b> <i>Time Constant Equations</i>
</p>

We can then set *lambda = t* and calculate *K = dRPM/dPWM* and plug that into our *ki* and *kp* PID parameters.

<div align="center">
  <img src="../images/Lambda Tuning Example.png" alt="Lambda Tuning Ex" width="300">
</div>

<p align="center">
  <b>Figure 3:</b> <i>Lambda Tuning Example</i>
</p>

The plan is to repeat this process at different pwm changes and save that information to the flash in the form of a lookup table for PI perameters at
different rpm requests.
