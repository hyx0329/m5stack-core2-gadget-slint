use std::sync::mpsc;
use std::{thread, thread::JoinHandle};

use axp2101::Axp2101;
use embedded_hal::i2c::I2c;
use esp_idf_svc::hal::delay::FreeRtos as FreeRtosDelay;
use esp_idf_svc::hal::gpio::{Input, InputPin, InterruptType, PinDriver};
use ft6336::Ft6336;

use crate::utils::block_for_interrupt;

use super::InputEvent;

/// The thread for touch events processing.
#[inline]
pub fn touch_event_task<I2C, PIN>(
    mut touch_panel: Ft6336<I2C>,
    mut touch_interrupt: PinDriver<'static, PIN, Input>,
    sender: mpsc::SyncSender<InputEvent>,
) -> JoinHandle<()>
where
    I2C: I2c + Send + 'static,
    PIN: InputPin,
{
    // Touch panel on M5Stack Core2 has correct physical cordinates.
    // visible space: (0..320), (0..240)
    // touch buttons: (0..320), (240..280)
    thread::spawn(move || {
        touch_panel.init().unwrap();
        touch_panel.interrupt_by_pulse().unwrap();
        loop {
            block_for_interrupt(&mut touch_interrupt, InterruptType::NegEdge);
            for p in touch_panel.touch_points_iter().unwrap() {
                log::info!("Point: {:?}", p);
            }
            FreeRtosDelay::delay_ms(20);
        }
    })
}

/// The thread for PMU events processing.
#[inline]
pub fn pmu_event_task<I2C, PIN>(
    mut pmu: Axp2101<I2C>,
    mut pmu_interrupt: PinDriver<'static, PIN, Input>,
    sender: mpsc::SyncSender<InputEvent>,
) -> JoinHandle<()>
where
    I2C: I2c + Send + 'static,
    PIN: InputPin,
{
    thread::spawn(move || {
        loop {
            block_for_interrupt(&mut pmu_interrupt, InterruptType::LowLevel);
            log::debug!("NEW PMU IRQ event(s) detected!");
            // get current events
            let mut events = pmu.irq_status().unwrap();
            // clear the flags first
            pmu.irq_clear_all().unwrap();
            // FIXME: Mask/skip some unused interrupts, how to do it properly?
            events.1 &= 0b11111100;
            events.2 &= 0b01011111;
            for event in events.into_iter() {
                let _ = sender.send(InputEvent::Pmu(event));
            }
            FreeRtosDelay::delay_ms(50);
        }
    })
}
