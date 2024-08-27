use esp32_nimble::{enums::*, BLEDevice};
use esp_idf_svc::hal::delay::FreeRtos as FreeRtosDelay;
use std::{
    sync::mpsc::{self, SyncSender},
    thread,
};

use super::devices::*;

/// Simple task control commands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JuicyTaskControl {
    Start,
    Stop,
    SetPower(u8),
    Terminate,
}

fn to_power_level(value: u8) -> PowerLevel {
    match value {
        0 => PowerLevel::N12,
        1 => PowerLevel::N9,
        2 => PowerLevel::N6,
        3 => PowerLevel::N3,
        4 => PowerLevel::N0,
        5 => PowerLevel::P3,
        6 => PowerLevel::P6,
        _ => PowerLevel::P9,
    }
}

fn rand_u32() -> u32 {
    unsafe { esp_idf_svc::sys::esp_random() }
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
        ble_device
            // maximum power on esp32, there are higher levels on some of other chips
            .set_power(PowerType::Advertising, PowerLevel::P9)
            .unwrap();
        let mut max_power_level: u8 = 7;

        let mut task_running = false;

        loop {
            for event in receiver.try_iter() {
                match event {
                    JuicyTaskControl::Start => task_running = true,
                    JuicyTaskControl::Stop => task_running = false,
                    JuicyTaskControl::Terminate => return,
                    JuicyTaskControl::SetPower(value) => {
                        max_power_level = value;
                    }
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

            // choose a device raw advertising data
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
            let raw_address: [u8; 6] = [
                // random address, most 2 significant bits must both be 1
                // it's a bluetooth standard
                ((random_number >> 16) | 0xC0) as u8,
                (random_number >> 24) as u8,
                random_number_2 as u8,
                (random_number_2 >> 8) as u8,
                (random_number_2 >> 16) as u8,
                (random_number_2 >> 24) as u8,
            ];

            let conn_mode = match random_number_2 & 0b1 {
                // only these 2 are valid in this usecase
                0 => ConnMode::Non,
                _ => ConnMode::Und,
            };

            // apply settings
            ble_device.set_rnd_addr(raw_address).unwrap();
            ble_device
                .set_power(PowerType::Advertising, power_level)
                .unwrap();
            ble_advertising
                .lock()
                .advertisement_type(conn_mode)
                .set_raw_data(raw_adv_data)
                .unwrap();

            // run advertisement
            ble_advertising.lock().start().unwrap();
            // spare time for other tasks, also being the advertising duration.
            FreeRtosDelay::delay_ms(1000);
            ble_advertising.lock().stop().unwrap();
            FreeRtosDelay::delay_ms(50);
        }
    });

    sender
}
