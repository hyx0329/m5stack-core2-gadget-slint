use std::{cell::RefCell, rc::Rc, time};
use slint::platform::{Platform, software_renderer::MinimalSoftwareWindow};

static INITIAL_INSTANT: once_cell::sync::OnceCell<time::Instant> = once_cell::sync::OnceCell::new();

pub struct M5Core2V11GadgetPlatform {
    // pub window: RefCell<Option<Rc<slint::platform::software_renderer::MinimalSoftwareWindow>>>,
    pub window: Rc<MinimalSoftwareWindow>,
}

impl Platform for M5Core2V11GadgetPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        // // copied from esp32 mcu examples
        // let window = slint::platform::software_renderer::MinimalSoftwareWindow::new(
        //     slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
        // );
        // self.window.replace(Some(window.clone()));
        // Ok(window)

        // mcu doc example
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        // the implementation is copied from original std implementation XD
        let the_beginning = *INITIAL_INSTANT.get_or_init(time::Instant::now);
        time::Instant::now() - the_beginning
    }

    fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
        todo!()
    }
}
