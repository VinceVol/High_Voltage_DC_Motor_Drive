* SRS-0515-2 Isolated DC-DC Converter Behavioral Model
* Pins: +Vin -Vin +Vout -Vout
.subckt SRS0515 1 2 3 4
* 1: +Vin (Pin 1 & 16) 
* 2: -Vin (Pin 8 & 9) 
* 3: +Vout (Pin 7 & 10) 
* 4: -Vout (Pin 6 & 11) 

* Isolation: 10 Gigohm resistance and 80pF capacitance [cite: 57, 58]
R_iso 2 4 10G
C_iso 2 4 80p

* Input Current Sense (Approx 60% efficiency) [cite: 8, 76]
* Simulates the 5V to 15V conversion and draws current from input
G_pwr 1 2 value={ (V(3,4)*I(V_load)) / (V(1,2)*0.6) }

* Output Stage: 15V Regulated 
V_out_ideal 3 5 15V
* Internal Resistance to simulate 1% Line/Load regulation [cite: 18, 19]
R_out 5 4 0.5

* Current Limit: Approx 67mA 
.model D_lim D(Is=1e-12)
V_lim 5 6 0.7
D_clamp 3 6 D_lim
.ends SRS0515
