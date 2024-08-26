use std::{sync::mpsc::{self, SyncSender}, thread, time::Duration};
use esp32_nimble::{
    enums::*, BLEAddress, BLEAddressType, BLEAdvertisementData, BLECharacteristic, BLEDevice,
};
use esp_idf_svc::hal::delay::FreeRtos as FreeRtosDelay;

use super::devices::*;

/// Simple task control commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JuicyTaskControl {
    Start,
    Stop,
    SetPower(u8),
    Terminate,
}

fn to_power_level(value: u8) -> PowerLevel {
    match value {
        0 => PowerLevel::N0,
        1 => PowerLevel::N0,
        2 => PowerLevel::N0,
        3 => PowerLevel::N0,
        4 => PowerLevel::N0,
        5 => PowerLevel::P3,
        6 => PowerLevel::P6,
        _ => PowerLevel::P9,
    }
}

fn rand_u32() -> u32 {
    unsafe {
        esp_idf_svc::sys::esp_random()
    }
}

/// Spawn apple juice task and return a control handle.
#[inline]
pub fn spawn_applejuice_task() -> SyncSender<JuicyTaskControl> {
    let (sender, receiver) = mpsc::sync_channel::<JuicyTaskControl>(3);
    let _ = thread::spawn(move || {
        let ble_device = BLEDevice::take();
        ble_device.set_own_addr_type_to_non_resolvable_random();
        let _ble_server = ble_device.get_server();
        let ble_advertising = ble_device.get_advertising();

        // set default values
        ble_device.set_power(PowerType::Advertising, PowerLevel::P9).unwrap(); // maximum power(?)
        let mut max_power_level: u8 = 7;

        let mut task_running = false;

        loop {
            for event in receiver.try_iter() {
                match event {
                    JuicyTaskControl::Start => {task_running = true},
                    JuicyTaskControl::Stop => {task_running = false},
                    JuicyTaskControl::Terminate => return,
                    JuicyTaskControl::SetPower(value) => {
                        max_power_level = value;
                    },
                };
            }

            if !task_running {
                FreeRtosDelay::delay_ms(500);
                continue;
            }

            // TODO: randomize
            let power_level = to_power_level(max_power_level);

            // 4 bytes, utilize all randomness
            let random_number = rand_u32();

            // choose a device manufacturer data
            let raw_adv_data: &[u8] = {
                if (random_number >> 8) & 1 == 1 {
                    let index = random_number as usize % DEVICES.len();
                    &DEVICES[index][..]
                } else {
                    let index = random_number as usize % DEVICES_SHORT.len();
                    &DEVICES_SHORT[index][..]
                }
            };

            // choose a random address
            let random_number_2 = rand_u32();
            // the actual address is reversed
            let raw_address: [u8; 6] = [
                ((random_number >> 16) | 0xC0) as u8, // random address, bits 0,1 should both be 1
                (random_number >> 24) as u8,
                random_number_2 as u8,
                (random_number_2 >> 8) as u8,
                (random_number_2 >> 16) as u8,
                (random_number_2 >> 24) as u8,
            ];

            let conn_mode = match random_number_2 & 0b11 {
                0 => ConnMode::Non,
                _ => ConnMode::Und,
            };

            // apply settings
            ble_device.set_rnd_addr(raw_address).unwrap();
            ble_device.set_power(PowerType::Advertising, power_level).unwrap();
            ble_advertising.lock()
                .advertisement_type(conn_mode)
                .set_raw_data(raw_adv_data)
                .unwrap();

            ble_advertising.lock().start().unwrap();
            // spare time for other tasks, also being the advertising duration.
            FreeRtosDelay::delay_ms(1000);
            ble_advertising.lock().stop().unwrap();
            FreeRtosDelay::delay_ms(10);
        }
    });

    sender
}
