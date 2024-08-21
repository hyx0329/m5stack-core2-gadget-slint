use core::num::NonZero;
use esp_idf_svc::hal::{
    gpio::{Input, InputPin, InterruptType, PinDriver},
    task::notification::Notification,
};

/// block until the desired interrupt occurs on the given pin
#[inline]
pub fn block_for_interrupt<PIN>(pin: &mut PinDriver<'_, PIN, Input>, interrupt_type: InterruptType)
where
    PIN: InputPin,
{
    // prepare
    let notification = Notification::new();
    let waker = notification.notifier();
    pin.set_interrupt_type(interrupt_type).unwrap();
    // register the interrupt handler
    unsafe {
        pin.subscribe_nonstatic(move || {
            waker.notify(NonZero::new(1).unwrap());
        })
        .unwrap();
    }
    // enable interrupt, once
    pin.enable_interrupt().unwrap();
    // wait for notification
    notification.wait_any();
}
