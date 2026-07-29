use windows::UI::ViewManagement::{UIColorType, UISettings};
use windows_reactor::{
    border, Color, Element, ElementExt, HorizontalAlignment, PointerEventInfo, RenderCx, SetState,
};

pub const RESIZE_HANDLE_WIDTH: f64 = 6.0;

const TRANSPARENT: Color = Color { a: 0, r: 0, g: 0, b: 0 };
/// Fallback when `UISettings` is unavailable (the default Windows blue accent).
const FALLBACK_ACCENT: Color = Color { a: 255, r: 0, g: 120, b: 212 };

fn accent_color() -> Color {
    UISettings::new()
        .and_then(|s| s.GetColorValue(UIColorType::Accent))
        .map(|c| Color { a: c.A, r: c.R, g: c.G, b: c.B })
        .unwrap_or(FALLBACK_ACCENT)
}

pub struct ResizeHandle {
    hovered: bool,
    set_hovered: SetState<bool>,
    pressed: bool,
    set_pressed: SetState<bool>,
    current: f64,
    set: SetState<f64>,
    min: f64,
    max: f64,
}

pub fn resize_handle(cx: &mut RenderCx, current: f64, set: SetState<f64>) -> ResizeHandle {
    let (hovered, set_hovered) = cx.use_state(false);
    let (pressed, set_pressed) = cx.use_state(false);
    ResizeHandle { hovered, set_hovered, pressed, set_pressed, current, set, min: 0.0, max: f64::MAX }
}

impl ResizeHandle {
    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn build(self) -> Element {
        let Self { hovered, set_hovered, pressed, set_pressed, current, set, min, max } = self;
        let background = if pressed {
            accent_color()
        } else if hovered {
            Color { a: 128, ..accent_color() }
        } else {
            TRANSPARENT
        };
        let set_hovered_on_exit = set_hovered.clone();
        let set_pressed_on_release = set_pressed.clone();
        border(Element::Empty)
            .width(RESIZE_HANDLE_WIDTH)
            .background(background)
            .horizontal_alignment(HorizontalAlignment::Left)
            .on_pointer_entered(move |_: PointerEventInfo| set_hovered.call(true))
            .on_pointer_exited(move || set_hovered_on_exit.call(false))
            .on_pointer_pressed(move |_: PointerEventInfo| set_pressed.call(true))
            .on_pointer_released(move |_: PointerEventInfo| set_pressed_on_release.call(false))
            .on_pointer_moved(move |info: PointerEventInfo| {
                if info.is_left_button_pressed {
                    set.call((current + info.x).clamp(min, max));
                }
            })
            .into()
    }
}
