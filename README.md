# CandyPi - Lightning-Paid Candy Dispenser

A Rust application for a Raspberry Pi Zero 2 W that displays QR codes on an ST7735 LCD display and controls a motor for dispensing candy.

## Hardware Setup

### Display Wiring (ST7735 LCD)
- LED → GPIO 22 (pin 15)
- SCK → GPIO 11 SCLK (pin 23)
- SDA → GPIO 10 MOSI (pin 19)
- A0 (DC) → GPIO 24 (pin 18)
- RESET → GPIO 25 (pin 22)
- CS → GPIO 8 CE0 (pin 24)
- GND → Ground (pin 9)
- VCC → 3.3V (pin 1)

### Motor
- Motor control → GPIO 4

## Features
- Displays a QR code (currently a test Lightning invoice string)
- Controls motor for candy dispensing (2-second duration)
- CLI interface - press Enter to dispense candy
- Clean shutdown on Ctrl+C

## Building
```bash
cargo build --release
```

## Running
```bash
sudo ./target/release/candypi
```
Note: Requires sudo for GPIO access on Raspberry Pi.

## Configuration
- `MOTOR_DISPENSE_DURATION_MS`: Motor run time in milliseconds (default: 2000)
- `TEST_QR_STRING`: QR code content (currently a test Lightning invoice)
- Display dimensions: 128x160 pixels

## Future Enhancements
- Replace test QR string with actual Lightning invoice generation
- Add payment verification before dispensing
- Configure dispense duration via config file or CLI args