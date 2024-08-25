use embedded_graphics_core::{
    pixelcolor::raw::RawU16,
    prelude::{DrawTarget, Point, Size},
    primitives::Rectangle,
};
use slint::platform::{software_renderer, software_renderer::MinimalSoftwareWindow, Platform};
use std::{rc::Rc, time};

static INITIAL_INSTANT: once_cell::sync::OnceCell<time::Instant> = once_cell::sync::OnceCell::new();

pub struct M5Core2V11GadgetPlatform {
    // pub window: RefCell<Option<Rc<slint::platform::software_renderer::MinimalSoftwareWindow>>>,
    pub window: Rc<MinimalSoftwareWindow>,
}

impl Platform for M5Core2V11GadgetPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
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
}

// simple display wrapper from the official example
pub struct DisplayWrapper<'a, T> {
    display: &'a mut T,
    line_buffer: &'a mut [software_renderer::Rgb565Pixel],
}

impl<'a, T> DisplayWrapper<'a, T> {
    pub fn new(display: &'a mut T, line_buffer: &'a mut [software_renderer::Rgb565Pixel]) -> Self {
        Self {
            display,
            line_buffer,
        }
    }
}

impl<T> software_renderer::LineBufferProvider for DisplayWrapper<'_, T>
where
    T: DrawTarget<Color = embedded_graphics_core::pixelcolor::Rgb565>,
{
    type TargetPixel = software_renderer::Rgb565Pixel;
    fn process_line(
        &mut self,
        line: usize,
        range: core::ops::Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        // Render into the line
        render_fn(&mut self.line_buffer[range.clone()]);

        // Send the line to the screen using DrawTarget::fill_contiguous
        self.display
            .fill_contiguous(
                &Rectangle::new(
                    Point::new(range.start as _, line as _),
                    Size::new(range.len() as _, 1),
                ),
                self.line_buffer[range.clone()]
                    .iter()
                    .map(|p| RawU16::new(p.0).into()),
            )
            .map_err(drop)
            .unwrap();
    }
}
