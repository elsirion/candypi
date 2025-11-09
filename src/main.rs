use rppal::gpio::{Gpio, OutputPin};
use rppal::hal::Delay;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi, SimpleHalSpiDevice};
use st7735_lcd::{Orientation, ST7735};
use qrcode::QrCode;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Rectangle, PrimitiveStyleBuilder},
    text::Text,
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    image::{Image, ImageRaw},
};
use std::io::{self, BufRead};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::thread;
use std::net::UdpSocket;

const MOTOR_PIN: u8 = 4;
const MOTOR_DISPENSE_DURATION_MS: u64 = 500;

const LCD_LED_PIN: u8 = 22;
const LCD_DC_PIN: u8 = 24;
const LCD_RST_PIN: u8 = 25;

const DISPLAY_WIDTH: u32 = 128;
const DISPLAY_HEIGHT: u32 = 160;

type Display = ST7735<SimpleHalSpiDevice<Spi>, OutputPin, OutputPin>;

struct DisplayLayout {
    qr_size: u32,
    qr_y_offset: u32,
    amount_y: u32,
    ip_y: u32,
}

impl DisplayLayout {
    fn new() -> Self {
        let qr_size = DISPLAY_WIDTH - 4; // Leave 2px margin on each side
        let qr_y_offset = 4; // Small margin from top
        let amount_y = qr_y_offset + qr_size + 8; // 8px below QR
        let ip_y = amount_y + 16; // 16px below amount
        
        Self {
            qr_size,
            qr_y_offset,
            amount_y,
            ip_y,
        }
    }
}

fn get_local_ip() -> String {
    match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => {
            if socket.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = socket.local_addr() {
                    return addr.ip().to_string();
                }
            }
        }
        Err(_) => {}
    }
    "No IP".to_string()
}

fn clear_display(display: &mut Display) {
    let bg = Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT))
        .into_styled(PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::BLACK)
            .build());
    let _ = bg.draw(display);
}

fn generate_qr_image(data: &str, target_size: u32) -> Result<(Vec<u8>, u32), Box<dyn std::error::Error>> {
    // Generate QR code with minimal border
    let code = QrCode::new(data)?;
    let qr_modules = code.width() as u32;
    
    // Calculate scale to fit nicely within target size
    let scale = (target_size / qr_modules).max(1);
    let actual_size = qr_modules * scale;
    
    // Create RGB565 image buffer manually for clean, square modules
    let mut qr_data = Vec::with_capacity((actual_size * actual_size * 2) as usize);
    
    for y in 0..actual_size {
        for x in 0..actual_size {
            let module_x = x / scale;
            let module_y = y / scale;
            
            let is_dark = if module_x < qr_modules && module_y < qr_modules {
                code[(module_x as usize, module_y as usize)] == qrcode::Color::Dark
            } else {
                false // White border if outside QR bounds
            };
            
            let rgb565 = if is_dark { 0x0000u16 } else { 0xFFFFu16 };
            qr_data.push((rgb565 & 0xFF) as u8);      // Low byte
            qr_data.push((rgb565 >> 8) as u8);        // High byte
        }
    }
    
    Ok((qr_data, actual_size))
}

fn display_invoice_screen(display: &mut Display, invoice_data: &str, amount: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Generating invoice display for: {}", invoice_data);
    
    let layout = DisplayLayout::new();
    
    // Clear screen with white background
    let bg = Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT))
        .into_styled(PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::WHITE)
            .build());
    let _ = bg.draw(display);
    
    // Generate QR code image
    let (qr_data, actual_qr_size) = generate_qr_image(invoice_data, layout.qr_size)?;
    
    let qr_x_offset = (DISPLAY_WIDTH - actual_qr_size) / 2;
    let qr_raw_image = ImageRaw::<Rgb565>::new(&qr_data, actual_qr_size);
    let qr_image_display = Image::new(&qr_raw_image, Point::new(qr_x_offset as i32, layout.qr_y_offset as i32));
    let _ = qr_image_display.draw(display);
    
    // Text styles
    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb565::BLACK);
    
    // Display amount below QR code
    let amount_text = Text::new(
        amount,
        Point::new(
            ((DISPLAY_WIDTH - (amount.len() as u32 * 6)) / 2) as i32, // Center text
            layout.amount_y as i32
        ),
        text_style,
    );
    let _ = amount_text.draw(display);
    
    // Display IP address
    let ip = get_local_ip();
    let ip_text = Text::new(
        &ip,
        Point::new(
            ((DISPLAY_WIDTH - (ip.len() as u32 * 6)) / 2) as i32, // Center text
            layout.ip_y as i32
        ),
        text_style,
    );
    let _ = ip_text.draw(display);
    
    println!("Invoice screen displayed!");
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
    display.set_orientation(&Orientation::PortraitSwapped).map_err(|_| "Failed to set orientation")?;
    
    // Initialize motor
    let mut motor_pin = gpio.get(MOTOR_PIN)?.into_output();
    motor_pin.set_low();
    
    // Display initial invoice screen
    let initial_invoice = generate_invoice_string();
    display_invoice_screen(&mut display, &initial_invoice, "42 sats")?;
    
    println!("Press Enter to dispense candy (Ctrl+C to exit)...");
    
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    
    loop {
        match lines.next() {
            Some(Ok(_)) => {
                match dispense_candy(&mut motor_pin) {
                    Ok(_) => {
                        // Generate and display new invoice screen for next purchase
                        let new_invoice = generate_invoice_string();
                        display_invoice_screen(&mut display, &new_invoice, "42 sats")?;
                        
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