use axp2101::irq::IrqReason as AxpIrqReason;
use ft6336::touch::{Point as FtPoint, PointAction};
use slint::{
    platform::{PointerEventButton, WindowEvent},
    LogicalPosition,
};

pub mod tasks;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum PointState {
    Pressed,
    #[default]
    Released,
    Moved,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Point {
    pub id: u8,
    pub state: PointState,
    pub x: u16,
    pub y: u16,
}

impl From<FtPoint> for Point {
    fn from(value: FtPoint) -> Self {
        Self {
            id: value.index,
            state: match value.action {
                PointAction::PressDown => PointState::Pressed,
                PointAction::Contact => PointState::Moved,
                _ => PointState::Released,
            },
            x: value.x,
            y: value.y,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum InputEvent {
    WindowEvent(WindowEvent),
    Pmu(AxpIrqReason),
}

impl From<Point> for WindowEvent {
    fn from(value: Point) -> Self {
        match value.state {
            PointState::Pressed => Self::PointerPressed {
                position: LogicalPosition::new(value.x as f32, value.y as f32),
                button: PointerEventButton::Left,
            },
            PointState::Released => todo!(),
            PointState::Moved => todo!(),
        }
    }
}
