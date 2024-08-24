use std::sync::mpsc;
use std::{thread, thread::JoinHandle};

use axp2101::Axp2101;
use embedded_hal::i2c::I2c;
use esp_idf_svc::hal::delay::FreeRtos as FreeRtosDelay;
use esp_idf_svc::hal::gpio::{Input, InputPin, InterruptType, PinDriver};
use ft6336::Ft6336;
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{LogicalPosition, SharedString};

use crate::utils::block_for_interrupt;

use super::InputEvent;

const TOUCH_BTN_LEFT: slint::platform::Key = slint::platform::Key::F1;
const TOUCH_BTN_CENTER: slint::platform::Key = slint::platform::Key::F2;
const TOUCH_BTN_RIGHT: slint::platform::Key = slint::platform::Key::F3;

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
    /*
    Touch panel on M5Stack Core2 has correct physical cordinates.
    visible space: (0..320), (0..240)
    touch buttons: (0..320), (240..280)
    Those 2 regions are just logical zones in software.

    The design of slint doesn't allow multiple touches.
    And because of this, only one point is allowed in the visible space(gesture region).

    Due to a hardware limitation, the touch panel cannot distinguish touches with equal
    Y-axis cordinates.
    The touch controller cannot handle multi-touch accurately anyway.
    */

    thread::spawn(move || {
        touch_panel.init().unwrap();
        touch_panel.interrupt_by_pulse().unwrap();
        loop {
            block_for_interrupt(&mut touch_interrupt, InterruptType::NegEdge);

            // when interrupt triggered, enter polling mode, until all released.
            // maximum 2 touches, and is also ensured by touch driver
            let mut last_status: [bool; 2] = [false; 2]; // track last pressing status
            let mut last_position: [(u16, u16); 2] = [(0, 0); 2]; // track last position, used when point released
            let mut pointer_index: Option<u8> = None; // track which point is for gestures

            loop {
                let points_iter = touch_panel.touch_points_iter().unwrap();

                // track point ids processed
                let mut processed: [bool; 2] = [false; 2];

                // all points in this iter are actively touched points,
                // this behavior is consistent with touch panel's firmware.
                for point in points_iter {
                    processed[point.index as usize] = true;

                    if point.y < 240 {
                        if pointer_index == None && !(last_status[point.index as usize]) {
                            // mark the first *new* valid point as pointer
                            pointer_index = Some(point.index);
                        }

                        if pointer_index != Some(point.index) {
                            // skip non-pointer point update
                            continue;
                        }

                        if (point.x, point.y) == last_position[point.index as usize] {
                            // skip dulplicated events
                            continue;
                        }

                        // update pointer location
                        let position = LogicalPosition::new(point.x as f32, point.y as f32);
                        // press/move based on last status
                        let new_pointer_event = if last_status[point.index as usize] {
                            // already activated
                            WindowEvent::PointerMoved { position }
                        } else {
                            WindowEvent::PointerPressed {
                                position,
                                button: PointerEventButton::Left,
                            }
                        };
                        // the scaling factor is 1 so no conversion
                        last_position[point.index as usize] = (point.x, point.y);
                        last_status[point.index as usize] = true;
                        sender
                            .send(InputEvent::WindowEvent(new_pointer_event))
                            .unwrap();
                    } else {
                        if pointer_index == Some(point.index) {
                            // skip updating pointer point
                            continue;
                        }

                        if !(last_status[point.index as usize]) {
                            // only update position once
                            last_status[point.index as usize] = true;
                            last_position[point.index as usize] = (point.x, point.y);

                            // touch buttons, map to keys rather than pointer events
                            // 320px sliced to 3 buttons
                            let new_key_event = if (0..107).contains(&point.x) {
                                WindowEvent::KeyPressed {
                                    text: TOUCH_BTN_LEFT.into(),
                                }
                            } else if (107..214).contains(&point.x) {
                                WindowEvent::KeyPressed {
                                    text: TOUCH_BTN_CENTER.into(),
                                }
                            } else {
                                WindowEvent::KeyPressed {
                                    text: TOUCH_BTN_RIGHT.into(),
                                }
                            };

                            sender.send(InputEvent::WindowEvent(new_key_event)).unwrap();
                        }
                    }
                }

                // release all un-processed(inactive) touches which was active before
                for (i, (last_known_status, _)) in last_status
                    .iter_mut()
                    .zip(processed.iter()) // consumed
                    .enumerate()
                    .filter(|x| *x.1 .0 && !*x.1 .1)
                {
                    *last_known_status = false; // don't forget marking it as inactive

                    if pointer_index == Some(i as u8) {
                        // release pointer event
                        pointer_index = None; // also clear pointer_index, required if N > 2
                        let position = LogicalPosition::new(
                            last_position[i].0 as f32,
                            last_position[i].1 as f32,
                        );
                        let release_event = WindowEvent::PointerReleased {
                            position,
                            button: PointerEventButton::Left,
                        };
                        sender.send(InputEvent::WindowEvent(release_event)).unwrap();
                        // this is required for hover effects to work properly
                        sender
                            .send(InputEvent::WindowEvent(WindowEvent::PointerExited))
                            .unwrap();
                    } else {
                        // release key
                        let release_event = if (0..107).contains(&last_position[i].0) {
                            WindowEvent::KeyReleased {
                                text: TOUCH_BTN_LEFT.into(),
                            }
                        } else if (107..214).contains(&last_position[i].0) {
                            WindowEvent::KeyReleased {
                                text: TOUCH_BTN_CENTER.into(),
                            }
                        } else {
                            WindowEvent::KeyReleased {
                                text: TOUCH_BTN_RIGHT.into(),
                            }
                        };
                        sender.send(InputEvent::WindowEvent(release_event)).unwrap();
                    };
                }

                // wait for touch panel's update
                // default update interval is approx. 19ms
                // always keep this delay to avoid triggering WDT
                FreeRtosDelay::delay_ms(20);

                // check if actually no new event
                if processed.into_iter().all(|x| !x) {
                    // into interrupt mode
                    break;
                }
            }
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
