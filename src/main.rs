use rppal::gpio::{Gpio, OutputPin};
use rppal::hal::Delay;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi, SimpleHalSpiDevice};
use st7735_lcd::{Orientation, ST7735};
use qrcode::{QrCode, Color};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Rectangle, PrimitiveStyleBuilder},
};
use std::io::{self, BufRead};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::thread;

const MOTOR_PIN: u8 = 4;
const MOTOR_DISPENSE_DURATION_MS: u64 = 2000;

const LCD_LED_PIN: u8 = 22;
const LCD_DC_PIN: u8 = 24;
const LCD_RST_PIN: u8 = 25;

const DISPLAY_WIDTH: u32 = 128;
const DISPLAY_HEIGHT: u32 = 160;

type Display = ST7735<SimpleHalSpiDevice<Spi>, OutputPin, OutputPin>;

fn clear_display(display: &mut Display) {
    let bg = Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT))
        .into_styled(PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::BLACK)
            .build());
    let _ = bg.draw(display);
}

fn display_qr_code(display: &mut Display, data: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Generating QR code for: {}", data);
    
    let code = QrCode::new(data)?;
    
    let style_black = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::BLACK)
        .build();
    
    let style_white = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::WHITE)
        .build();
    
    // Clear with white background
    let bg = Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT))
        .into_styled(style_white);
    let _ = bg.draw(display);
    
    let qr_size = code.width() as u32;
    let scale = ((DISPLAY_WIDTH.min(DISPLAY_HEIGHT) - 20) / qr_size).max(1);
    let scaled_size = qr_size * scale;
    
    let x_offset = (DISPLAY_WIDTH - scaled_size) / 2;
    let y_offset = (DISPLAY_HEIGHT - scaled_size) / 2;
    
    // Draw QR code
    for y in 0..qr_size {
        for x in 0..qr_size {
            let color = if code[(x as usize, y as usize)] == Color::Dark {
                style_black
            } else {
                style_white
            };
            
            let rect = Rectangle::new(
                Point::new(
                    (x_offset + x * scale) as i32,
                    (y_offset + y * scale) as i32
                ),
                Size::new(scale, scale)
            ).into_styled(color);
            
            let _ = rect.draw(display);
        }
    }
    
    println!("QR code displayed!");
    Ok(())
}

fn generate_invoice_string() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    // Generate a unique invoice string for each dispense
    // In production, this would be a real Lightning invoice
    format!("lightning:lnbc100u1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdpl2p{}", 
            timestamp)
}

fn dispense_candy(motor_pin: &mut OutputPin) -> Result<(), Box<dyn std::error::Error>> {
    println!("Dispensing candy for {} ms...", MOTOR_DISPENSE_DURATION_MS);
    
    motor_pin.set_high();
    thread::sleep(Duration::from_millis(MOTOR_DISPENSE_DURATION_MS));
    motor_pin.set_low();
    
    println!("Candy dispensed!");
    
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Initializing Candy Dispenser...");
    
    let gpio = Gpio::new()?;
    
    // Initialize SPI and display
    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 16_000_000, Mode::Mode0)?;
    let spi_device = SimpleHalSpiDevice::new(spi);
    
    let dc_pin = gpio.get(LCD_DC_PIN)?.into_output();
    let rst_pin = gpio.get(LCD_RST_PIN)?.into_output();
    let mut led_pin = gpio.get(LCD_LED_PIN)?.into_output();
    led_pin.set_high();
    
    let mut display = ST7735::new(spi_device, dc_pin, rst_pin, false, false, DISPLAY_WIDTH, DISPLAY_HEIGHT);
    
    let mut delay = Delay::new();
    display.init(&mut delay).map_err(|_| "Failed to initialize display")?;
    display.set_orientation(&Orientation::Portrait).map_err(|_| "Failed to set orientation")?;
    
    // Initialize motor
    let mut motor_pin = gpio.get(MOTOR_PIN)?.into_output();
    motor_pin.set_low();
    
    // Display initial QR code
    let initial_invoice = generate_invoice_string();
    display_qr_code(&mut display, &initial_invoice)?;
    
    println!("Press Enter to dispense candy (Ctrl+C to exit)...");
    
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    
    loop {
        match lines.next() {
            Some(Ok(_)) => {
                match dispense_candy(&mut motor_pin) {
                    Ok(_) => {
                        // Generate and display new QR code for next purchase
                        let new_invoice = generate_invoice_string();
                        display_qr_code(&mut display, &new_invoice)?;
                        
                        println!("Ready for next dispense. Press Enter to dispense again...");
                    }
                    Err(e) => {
                        eprintln!("Error during dispensing: {}", e);
                    }
                }
            }
            Some(Err(e)) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
            None => {
                println!("End of input stream");
                break;
            }
        }
    }
    
    // Cleanup
    println!("Shutting down...");
    motor_pin.set_low();
    led_pin.set_low();
    clear_display(&mut display);
    
    Ok(())
}