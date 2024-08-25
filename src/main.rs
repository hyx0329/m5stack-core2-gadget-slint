use esp_idf_svc::hal::{
    delay::{Ets as EtsDelay, FreeRtos as FreeRtosDelay},
    gpio::{PinDriver, Pull},
    i2c,
    peripherals::Peripherals,
    spi,
    units::FromValueType as _,
};
use std::{
    boxed::Box,
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};

use display_interface_spi::SPIInterface;
use embedded_hal_bus::i2c::MutexDevice as SharedI2cBus;
use mipidsi::{
    models::ILI9342CRgb565,
    options::{ColorInversion, ColorOrder},
    Builder as MipiBuilder,
};

use axp2101::{Aldo2, Axp2101, Bldo1, Dcdc1, Regulator as _, RegulatorPin};
use ft6336::Ft6336;
use ina3221::Ina3221;
use mpu6886::Mpu6886;
use pcf8563::Pcf8563;

mod platform;
mod utils;
// TODOs
mod applejuice;
mod inputevent;

use inputevent::{
    tasks::{pmu_event_task, touch_event_task},
    InputEvent,
};

use platform::{DisplayWrapper, M5Core2V11GadgetPlatform};
use slint::platform::software_renderer::MinimalSoftwareWindow;

slint::include_modules!();

const INPUT_BUFFER_SIZE: usize = 32;

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Initializing peripherals...");

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
    let mut touch_panel = Ft6336::new(SharedI2cBus::new(mutex_i2c_bus));
    let mut inertial = Mpu6886::new(SharedI2cBus::new(mutex_i2c_bus));
    let mut voltmon = Ina3221::new(SharedI2cBus::new(mutex_i2c_bus));

    // check axp status and turn on 3V3 bus
    {
        match pmu.chip_id() {
            Ok(chip_id) => log::info!("AXP2101 found, ID {}", chip_id),
            Err(e) => panic!("AXP2101 initialization failure! {:?}", e),
        };
        // 3.3V dcdc1 for esp32
        let mut dcdc1 = Dcdc1::new(SharedI2cBus::new(mutex_i2c_bus));
        dcdc1.set_voltage(3300).unwrap();
        dcdc1.enable().unwrap();
        // 3.3v dcdc3 for esp32 and peripherals
        let mut dcdc3 = Dcdc1::new(SharedI2cBus::new(mutex_i2c_bus));
        dcdc3.set_voltage(3300).unwrap();
        dcdc3.enable().unwrap();
    };

    // Initialize SPI, allocated at runtime
    let spi_bus = {
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
        Box::leak(spi_bus_boxed)
    };

    // TODO: SD card on the SPI bus
    // TODO: SD mount/unmount
    // let tfcard_cs = peripherals.pins.gpio4;

    // LCD on the SPI bus
    let mut display = {
        // 40Mhz is the maximum stable & available freq
        let display_spi_config = spi::SpiConfig::new()
            .duplex(spi::config::Duplex::Half)
            .baudrate(40u32.MHz().into());
        let lcd_cs = peripherals.pins.gpio5;
        let display_spi_bus =
            spi::SpiDeviceDriver::new(spi_bus, Some(lcd_cs), &display_spi_config).unwrap();
        let aldo2 = Aldo2::new(SharedI2cBus::new(mutex_i2c_bus));
        let lcd_rst = RegulatorPin::new(aldo2);
        let lcd_dc = PinDriver::output(peripherals.pins.gpio15).unwrap();
        let display_interface = SPIInterface::new(display_spi_bus, lcd_dc);
        MipiBuilder::new(ILI9342CRgb565, display_interface)
            .reset_pin(lcd_rst)
            .display_size(320, 240)
            .color_order(ColorOrder::Bgr)
            .invert_colors(ColorInversion::Inverted)
            .init(&mut EtsDelay)
            .unwrap()
    };
    let mut lcd_backlight = Bldo1::new(SharedI2cBus::new(mutex_i2c_bus));

    // display prefilling, backlight on
    // TODO: handle error properly
    // display.clear(Rgb565::BLACK).unwrap();
    lcd_backlight.set_voltage(2800).unwrap();
    lcd_backlight.enable().unwrap();

    let psram_initialized: bool = unsafe { esp_idf_svc::sys::esp_psram_is_initialized() };
    log::info!("PSRAM initialized: {}", psram_initialized);
    let psram_size: usize = unsafe { esp_idf_svc::sys::heap_caps_get_free_size(esp_idf_svc::sys::MALLOC_CAP_SPIRAM) };
    log::info!("Available PSRAM size(approx.): {}KB", psram_size / 1024);

    log::info!("Initializing input sources...");

    // communication channel / event queue
    let (inputevent_tx, inputevent_rx) = mpsc::sync_channel::<InputEvent>(INPUT_BUFFER_SIZE);
    let inputevent_tx_pmu = inputevent_tx.clone();
    let inputevent_tx_touch = inputevent_tx;

    // thread for reading PMU events
    let mut pmu_interrupt = PinDriver::input(peripherals.pins.gpio19).unwrap();
    pmu_interrupt.set_pull(Pull::Up).unwrap();
    let _t_input_pmu = pmu_event_task(pmu, pmu_interrupt, inputevent_tx_pmu);

    // thread for reading touch events
    let touch_interrupt = PinDriver::input(peripherals.pins.gpio39).unwrap();
    let _t_input_touch = touch_event_task(touch_panel, touch_interrupt, inputevent_tx_touch);

    log::info!("Initializing slint...");

    // slint init
    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
    );
    slint::platform::set_platform(Box::new(M5Core2V11GadgetPlatform {
        window: window.clone(),
    }))
    .unwrap();

    // prepare buffer and configure root window size
    let mut line_buffer = [slint::platform::software_renderer::Rgb565Pixel(0); 320];
    window.set_size(slint::PhysicalSize::new(320, 240));

    // UI configuration
    // This is merely an app view, different from the window.
    let _app_ui = GadgetMainWindow::new().unwrap();

    // The event loop(super loop)
    log::info!("Starting super loop...");
    loop {
        slint::platform::update_timers_and_animations();

        for event in inputevent_rx.try_iter() {
            match event {
                InputEvent::WindowEvent(event) => window.dispatch_event(event),
                InputEvent::Pmu(event) => log::info!("PMU event: {:?}", event),
            }
        }

        window.draw_if_needed(|renderer| {
            renderer.render_by_line(DisplayWrapper::new(&mut display, &mut line_buffer));
        });

        // spare time for other services
        // so watchdog will be fed
        if window.has_active_animations() {
            // has active animation, but it's still required to spare some time for other idle tasks
            // lets say a minimum of 10ms(at 100Hz kernel tick frequency)
            FreeRtosDelay::delay_ms(10);
        } else {
            // no active animation, reduce refresh rate
            // here the slint timer is not considered, because the main loop model is based on polling
            FreeRtosDelay::delay_ms(50);
        }
    }
}
