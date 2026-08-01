use windows_reactor::{
    border, grid, Color, Element, ElementExt, GridLength, HookRef, HorizontalAlignment,
    PointerEventInfo, RenderCx, SetState, ThemeRef, VerticalAlignment,
};

/// Width of the full drag surface (the hit-test area, not the visible pill).
pub const RESIZE_HANDLE_WIDTH: f64 = 6.0;

/// Width of the visible pill, matching the NavigationView selection indicator.
const INDICATOR_WIDTH: f64 = 3.0;

const TRANSPARENT: Color = Color { a: 0, r: 0, g: 0, b: 0 };

/// Height of the visible pill indicator. The indicator is always centered
/// within the drag surface regardless of which variant is used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandleSize {
    /// Fraction of the drag surface's height, clamped to `0.0..=1.0`.
    Percent(f64),
    /// Fixed height in DIPs.
    Absolute(f64),
}

pub struct ResizeHandle {
    hovered: bool,
    set_hovered: SetState<bool>,
    pressed: bool,
    set_pressed: SetState<bool>,
    /// `(root_x, current)` captured on `PointerPressed` - the anchor for
    /// computing a real drag delta. See the `on_pointer_moved` comment in
    /// [`build`](Self::build) for why this can't just be `current + info.x`.
    drag_start: HookRef<(f64, f64)>,
    current: f64,
    set: SetState<f64>,
    min: f64,
    max: f64,
    indicator_size: HandleSize,
}

pub fn resize_handle(cx: &mut RenderCx, current: f64, set: SetState<f64>) -> ResizeHandle {
    let (hovered, set_hovered) = cx.use_state(false);
    let (pressed, set_pressed) = cx.use_state(false);
    let drag_start = cx.use_ref((0.0_f64, 0.0_f64));
    ResizeHandle {
        hovered,
        set_hovered,
        pressed,
        set_pressed,
        drag_start,
        current,
        set,
        min: 0.0,
        max: f64::MAX,
        indicator_size: HandleSize::Percent(0.24),
    }
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

    /// Height of the visible pill indicator. Defaults to `Percent(0.12)`.
    pub fn indicator_size(mut self, v: HandleSize) -> Self {
        self.indicator_size = v;
        self
    }

    pub fn build(self) -> Element {
        let Self {
            hovered,
            set_hovered,
            pressed,
            set_pressed,
            drag_start,
            current,
            set,
            min,
            max,
            indicator_size,
        } = self;

        // Fluent reserves the fully-saturated `Accent`/`AccentFillColorDefaultBrush`
        // for prominent controls (buttons, toggles); it reads as too loud for a
        // hairline handle. `AccentSecondary` is the muted token Windows itself
        // uses for this kind of surface-level affordance.
        //
        // The brush binding is deliberately *constant* across all three states
        // (never a Direct color). The reactor backend applies `ThemeRef` via a
        // WinUI `Style` Setter, but on `Unset` it clears the prop with a direct
        // `SetBackground(null)` rather than `ClearValue` - a local value (even
        // null) permanently outranks a Style Setter's `{ThemeResource ...}`, so
        // switching this prop between `Direct` and `Theme` across renders would
        // silently and irrecoverably prevent the theme brush from ever painting
        // again. Visibility is driven by `opacity` instead, which has no such
        // precedence trap.
        let indicator_opacity = if pressed {
            1.0
        } else if hovered {
            0.7
        } else {
            0.0
        };

        let pill = border(Element::Empty)
            .width(INDICATOR_WIDTH)
            .corner_radius(INDICATOR_WIDTH / 2.0)
            .background(ThemeRef::AccentSecondary)
            .opacity(indicator_opacity)
            .horizontal_alignment(HorizontalAlignment::Center);

        // Two equal `Star` rows on either side of the pill center it regardless
        // of the drag surface's actual height, without needing measured layout.
        let (top, mid, bottom) = match indicator_size {
            HandleSize::Percent(p) => {
                let p = p.clamp(0.0, 1.0);
                let side = ((1.0 - p) / 2.0).max(0.0);
                (GridLength::Star(side), GridLength::Star(p.max(0.0001)), GridLength::Star(side))
            }
            HandleSize::Absolute(px) => {
                (GridLength::Star(1.0), GridLength::Pixel(px.max(0.0)), GridLength::Star(1.0))
            }
        };

        let indicator = grid((
            Element::Empty.grid_row(0),
            Element::from(pill).grid_row(1),
            Element::Empty.grid_row(2),
        ))
        .rows([top, mid, bottom])
        .columns([GridLength::Star(1.0)]);

        let set_hovered_on_exit = set_hovered.clone();
        let set_pressed_on_release = set_pressed.clone();
        let drag_start_on_press = drag_start.clone();
        border(indicator)
            .width(RESIZE_HANDLE_WIDTH)
            // Background must stay set (even fully transparent) so the whole
            // drag surface hit-tests - a null background does not receive
            // pointer events in WinUI, only the visible pill would.
            .background(TRANSPARENT)
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Stretch)
            .on_pointer_entered(move |_: PointerEventInfo| set_hovered.call(true))
            .on_pointer_exited(move || set_hovered_on_exit.call(false))
            .on_pointer_pressed(move |info: PointerEventInfo| {
                set_pressed.call(true);
                // Anchor the drag to where it started, in `root_x` (window-
                // relative, not `element`-relative) - this handle's own
                // margin follows `current` every render, so its own local
                // coordinate origin moves out from under the drag on every
                // frame. `root_x` doesn't move with it.
                *drag_start_on_press.borrow_mut() = (info.root_x, current);
            })
            .on_pointer_released(move |_: PointerEventInfo| set_pressed_on_release.call(false))
            .on_pointer_moved(move |info: PointerEventInfo| {
                if info.is_left_button_pressed {
                    let (start_root_x, start_current) = *drag_start.borrow();
                    let delta = info.root_x - start_root_x;
                    set.call((start_current + delta).clamp(min, max));
                }
            })
            .into()
    }
}
