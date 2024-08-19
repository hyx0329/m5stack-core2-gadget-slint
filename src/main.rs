use std::{
    boxed::Box, sync::{mpsc, Mutex}, thread, time::Duration
};
use embedded_graphics_core::{pixelcolor::raw::RawU16, prelude::{DrawTarget, Point, Size}, primitives::Rectangle};
use esp_idf_svc::hal::{
    peripherals::Peripherals,
    delay::{Ets as EtsDelay, FreeRtos as FreeRtosDelay},
    gpio::{PinDriver, Pull},
    i2c, spi,
    units::FromValueType as _,
    task::block_on,
};

use embedded_hal_bus::i2c::MutexDevice as SharedI2cBus;
use display_interface_spi::SPIInterface;
use mipidsi::{options::{ColorInversion, ColorOrder}, models::ILI9342CRgb565, Builder as MipiBuilder};

use axp2101::{Axp2101, Aldo2, Bldo1};
use ft6336::Ft6336;
use ina3221::Ina3221;
use mpu6886::Mpu6886;
use pcf8563::Pcf8563;

mod platform;
use platform::M5Core2V11GadgetPlatform;
use slint::platform::software_renderer::MinimalSoftwareWindow;

slint::include_modules!();

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    // Initialize I2C and related devices
    let i2c_sda = peripherals.pins.gpio21;
    let i2c_scl = peripherals.pins.gpio22;
    let i2c_config = i2c::I2cConfig::default().baudrate(400u32.kHz().into());
    let i2c_bus = i2c::I2cDriver::new(peripherals.i2c0, i2c_sda, i2c_scl, &i2c_config).unwrap();
    let mutex_i2c_bus_boxed = Box::new(Mutex::new(i2c_bus));
    let mutex_i2c_bus = Box::leak(mutex_i2c_bus_boxed);

    // all built-in I2C devices
    let mut pmu = Axp2101::new(SharedI2cBus::new(mutex_i2c_bus));
    let mut rtc = Pcf8563::new(SharedI2cBus::new(mutex_i2c_bus));
    let mut touch = Ft6336::new(SharedI2cBus::new(mutex_i2c_bus));
    let mut inertial = Mpu6886::new(SharedI2cBus::new(mutex_i2c_bus));
    let mut voltmon = Ina3221::new(SharedI2cBus::new(mutex_i2c_bus));

    // Check AXP2101 status before proceeding.
    // match pmu.chip_id() {
    //     Ok(chip_id) => log::info!("AXP2101 found, ID {}", chip_id),
    //     Err(e) => panic!("AXP2101 initialization failure! {:?}", e),
    // };

    // Initialize SPI
    let spi_sdo = peripherals.pins.gpio23;
    let spi_sdi = peripherals.pins.gpio38;
    let spi_sck = peripherals.pins.gpio18;
    let spi_bus_boxed = Box::new(
        spi::SpiDriver::new(
            peripherals.spi3, // matching IOMUX VSPI, don't know if makes a difference
            spi_sck,
            spi_sdo,
            Some(spi_sdi),
            &spi::SpiDriverConfig::new(),
        )
        .unwrap(),
    );
    let spi_bus = Box::leak(spi_bus_boxed);

    // TODO: SD card on the SPI bus
    // TODO: SD mount/unmount
    // let tfcard_cs = peripherals.pins.gpio4;

    // LCD on the SPI bus
    // 40Mhz is the maximum stable & available freq
    let display_spi_config = spi::SpiConfig::new()
        .duplex(spi::config::Duplex::Half)
        .baudrate(40u32.MHz().into());
    let lcd_cs = peripherals.pins.gpio5;
    let display_spi_bus = spi::SpiDeviceDriver::new(spi_bus, Some(lcd_cs), &display_spi_config).unwrap();
    let lcd_rst = Aldo2::new(SharedI2cBus::new(mutex_i2c_bus));
    let lcd_dc = PinDriver::output(peripherals.pins.gpio15).unwrap();
    let mut lcd_backlight = Bldo1::new(SharedI2cBus::new(mutex_i2c_bus));
    let display_interface = SPIInterface::new(display_spi_bus, lcd_dc);
    let mut display = MipiBuilder::new(ILI9342CRgb565, display_interface)
        .reset_pin(lcd_rst)
        .display_size(320, 240)
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .init(&mut EtsDelay)
        .unwrap();

    // display prefilling, backlight on
    // TODO: handle error properly
    // display.clear(Rgb565::BLACK).unwrap();
    lcd_backlight.set_voltage(2800).unwrap();
    lcd_backlight.enable().unwrap();

    log::info!("Hello, world!");

    // slint init
    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer
    );
    slint::platform::set_platform(Box::new(M5Core2V11GadgetPlatform {
        window: window.clone(),
    })).unwrap();

    // this is a valid configuration order
    let mut line_buffer = [slint::platform::software_renderer::Rgb565Pixel(0); 320];
    window.set_size(slint::PhysicalSize::new(320, 240));

    let ui = GadgetMainWindow::new().unwrap();

    loop {
        window.draw_if_needed(|renderer| {
            renderer.render_by_line(DisplayWrapper {
                display: &mut display,
                line_buffer: &mut line_buffer,
            });
        });
        thread::sleep(Duration::from_millis(10));
    }
}

struct DisplayWrapper<'a, T>{
    display: &'a mut T,
    line_buffer: &'a mut [slint::platform::software_renderer::Rgb565Pixel],
}
impl<T: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>>
    slint::platform::software_renderer::LineBufferProvider for DisplayWrapper<'_, T>
{
    type TargetPixel = slint::platform::software_renderer::Rgb565Pixel;
    fn process_line(
        &mut self,
        line: usize,
        range: core::ops::Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        // Render into the line
        render_fn(&mut self.line_buffer[range.clone()]);

        // Send the line to the screen using DrawTarget::fill_contiguous
        self.display.fill_contiguous(
            &Rectangle::new(Point::new(range.start as _, line as _), Size::new(range.len() as _, 1)),
            self.line_buffer[range.clone()].iter().map(|p| RawU16::new(p.0).into())
        ).map_err(drop).unwrap();
    }
}
