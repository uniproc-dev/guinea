mod geometry;
mod ring;

pub use geometry::{bounds, nearest_point};
pub use ring::RingSeries;

use std::cell::Cell;
use std::rc::Rc;
use windows_canvas::{Brush, ColorF, GpuDevice, Path, PathBuilder, Rect, Result as CanvasResult, Vector2};

use crate::widgets::color::{hex, hex_alpha};
use windows_reactor::{CanvasSwapChain, DrawContext, Element, ElementExt, PointerEventInfo, RenderCx, swap_chain_panel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
    Smooth,
}

#[derive(Clone)]
pub struct Series {
    pub color: ColorF,
    pub interpolation: Interpolation,
    pub fill: Option<ColorF>,
    pub points: Vec<(u64, f32)>,
}

pub struct HoverInfo {
    pub x: u64,
    pub values: Vec<f32>,
}

#[derive(Clone, Copy, Debug)]
pub struct LineChartOptions {
    /// Solid background color; `None` for a fully transparent background.
    pub background: Option<ColorF>,
    /// Border color; `None` for no border. Use `theme_border_color()` to match
    /// the current dark/light scheme.
    pub border: Option<ColorF>,
    /// Whether to draw the background grid lines.
    pub show_grid: bool,
    /// Fixed Y range; `None` means auto-fit to the data.
    pub y_range: Option<(f32, f32)>,
}

impl Default for LineChartOptions {
    fn default() -> Self {
        Self {
            background: Some(BACKGROUND_TOP),
            border: Some(theme_border_color()),
            show_grid: true,
            y_range: None,
        }
    }
}

pub fn theme_border_color() -> ColorF {
    match windows_reactor::current_color_scheme() {
        windows_reactor::ColorScheme::Dark => hex_alpha(0xffffff, 36),
        windows_reactor::ColorScheme::Light => hex_alpha(0x000000, 36),
    }
}

const LINE_WIDTH: f32 = 1.0;
const GRID_LINE_WIDTH: f32 = 1.0;
const GRID_COLOR: ColorF = hex_alpha(0xffffff, 20);
const GRID_TARGET_SPACING_X: f32 = 80.0;
const GRID_TARGET_SPACING_Y: f32 = 40.0;
const BACKGROUND_TOP: ColorF = hex(0x1c1e26);
const BORDER_WIDTH: f32 = 1.0;

/// Cheap, comparable summary of a chart's data, used as a `use_effect`
/// dependency so the surface only redraws when the data actually changed
/// (grew or shifted), not on every unrelated re-render.
type ChartRevision = (usize, u64);

fn chart_revision(series: &[Series]) -> ChartRevision {
    let total_points: usize = series.iter().map(|s| s.points.len()).sum();
    let last_t = series.iter().filter_map(|s| s.points.last()).map(|&(t, _)| t).max().unwrap_or(0);
    (total_points, last_t)
}

pub fn line_chart(cx: &mut RenderCx, series: Vec<Series>, on_hover: impl Fn(Option<HoverInfo>) + 'static) -> Element {
    line_chart_with_options(cx, series, on_hover, LineChartOptions::default())
}

pub fn line_chart_with_options(
    cx: &mut RenderCx,
    series: Vec<Series>,
    on_hover: impl Fn(Option<HoverInfo>) + 'static,
    options: LineChartOptions,
) -> Element {
    // `CanvasSwapChain` (unlike `animated_canvas`) presents a frame only when
    // `draw` is called explicitly, instead of every vsync forever - a
    // continuous render loop for a chart that changes a few times a second
    // was burning several percent of a CPU core for nothing. So this reads
    // through a `use_ref` cell this function refreshes every render (the same
    // way `Router` persists itself across renders), and only actually calls
    // `draw` from a `use_effect` gated on `chart_revision`, i.e. when the
    // series data has actually grown.
    let series_cell = cx.use_ref(Vec::<Series>::new());
    series_cell.set(series);
    let options_cell = cx.use_ref(options);
    options_cell.set(options);

    let last_width = Rc::new(Cell::new(0.0f32));
    let chain_cell = cx.use_ref(None::<CanvasSwapChain>);
    let size_cell = Rc::new(Cell::new((0.0f32, 0.0f32)));

    let revision = chart_revision(&series_cell.borrow());
    let series_for_effect = series_cell.clone();
    let chain_for_effect = chain_cell.clone();
    let width_for_effect = last_width.clone();
    let options_for_effect = options_cell.clone();
    cx.use_effect((revision,), move || {
        if let Some(chain) = chain_for_effect.borrow().as_ref() {
            let series = series_for_effect.borrow();
            let options = *options_for_effect.borrow();
            let _ = chain.draw(|ctx| {
                width_for_effect.set(ctx.width);
                render(ctx, &series, &options);
            });
        }
    });

    let series_for_mount = series_cell.clone();
    let chain_for_mount = chain_cell.clone();
    let size_for_mount = size_cell.clone();
    let options_for_mount = options_cell.clone();
    let panel = swap_chain_panel().on_mounted(move |handle| {
        let scale = handle.composition_scale().map_or(1.0, |(x, _)| x);
        let (w, h) = size_for_mount.get();
        let Ok(chain) = CanvasSwapChain::new(&handle, w, h, scale) else {
            tracing::warn!("line_chart: failed to create swap chain on mount");
            return;
        };
        if w > 0.0 && h > 0.0 {
            let options = *options_for_mount.borrow();
            let _ = chain.draw(|ctx| render(ctx, &series_for_mount.borrow(), &options));
        }
        chain_for_mount.set(Some(chain));
    });

    let series_for_resize = series_cell.clone();
    let chain_for_resize = chain_cell.clone();
    let size_for_resize = size_cell.clone();
    let options_for_resize = options_cell.clone();
    let panel = panel.on_resize(move |w, h| {
        let (w, h) = (w as f32, h as f32);
        size_for_resize.set((w, h));
        if let Some(chain) = chain_for_resize.borrow().as_ref() {
            if chain.resize(w, h).is_ok() {
                let options = *options_for_resize.borrow();
                let _ = chain.draw(|ctx| render(ctx, &series_for_resize.borrow(), &options));
            }
        }
    });

    let chain_for_unmount = chain_cell.clone();
    let panel = panel.on_unmounted(move |_| chain_for_unmount.set(None));

    let on_hover = Rc::new(on_hover);

    let series_for_hover = series_cell.clone();
    let width_for_hover = last_width.clone();
    let on_hover_for_move = on_hover.clone();
    let panel = panel.on_pointer_moved(move |info: PointerEventInfo| {
        on_hover_for_move(hover_at(&series_for_hover.borrow(), info.x as f32, width_for_hover.get()));
    });

    let panel = panel.on_pointer_exited(move || on_hover(None));

    panel.into()
}

fn hover_at(series: &[Series], pixel_x: f32, width: f32) -> Option<HoverInfo> {
    let (min_t, max_t, _, _) = bounds(series)?;
    if width <= 0.0 || max_t == min_t {
        return None;
    }

    let ratio = (pixel_x / width).clamp(0.0, 1.0);
    let target = min_t + ((max_t - min_t) as f32 * ratio) as u64;

    let mut values = Vec::with_capacity(series.len());
    let mut any = false;
    for s in series {
        match nearest_point(&s.points, target) {
            Some((_, v)) => {
                values.push(v);
                any = true;
            }
            None => values.push(0.0),
        }
    }

    any.then_some(HoverInfo { x: target, values })
}

fn render(draw: &DrawContext<'_>, series: &[Series], options: &LineChartOptions) {
    draw.clear(ColorF::TRANSPARENT);

    let (width, height) = (draw.width, draw.height);
    if width <= 0.0 || height <= 0.0 {
        tracing::warn!(width, height, "line_chart: draw surface has zero size, skipping this frame");
        return;
    }

    if let Some(background) = options.background {
        draw_backdrop(draw, width, height, background);
    }
    if options.show_grid
        && let Ok(grid_brush) = draw.create_solid_brush(GRID_COLOR)
    {
        draw_grid(draw, &grid_brush, width, height);
    }
    if let Some(border) = options.border {
        let rect = Rect::from_xywh(0.0, 0.0, width, height);
        match draw.create_solid_brush(border) {
            Ok(brush) => draw.draw_rect(&rect, &brush, BORDER_WIDTH),
            Err(e) => tracing::warn!(error = %e, "line_chart: failed to create border brush"),
        }
    }

    let Some((min_t, max_t, min_v, max_v)) = bounds(series) else {
        tracing::trace!("line_chart: no points in any series yet, nothing to draw");
        return;
    };

    let (min_v, max_v) = options.y_range.unwrap_or((min_v, max_v));

    let t_span = (max_t - min_t).max(1) as f32;
    let v_span = (max_v - min_v).max(f32::EPSILON);

    let to_screen = move |t: u64, v: f32| -> Vector2 {
        let x = ((t - min_t) as f32 / t_span) * width;
        let y = height - ((v - min_v) / v_span) * height;
        Vector2 { x, y }
    };

    let device = draw.device();
    for s in series {
        if s.points.len() < 2 {
            tracing::trace!(points = s.points.len(), "line_chart: series has fewer than 2 points, skipping");
            continue;
        }

        if let Some(fill_color) = s.fill {
            match build_path(device, s, &to_screen, height, true) {
                Ok(path) => match draw.create_solid_brush(fill_color) {
                    Ok(brush) => draw.fill_path(&path, &brush),
                    Err(e) => tracing::warn!(error = %e, "line_chart: failed to create fill brush"),
                },
                Err(e) => tracing::warn!(error = %e, "line_chart: failed to build fill path"),
            }
        }

        match build_path(device, s, &to_screen, height, false) {
            Ok(path) => match draw.create_solid_brush(s.color) {
                Ok(brush) => draw.draw_path(&path, &brush, LINE_WIDTH),
                Err(e) => tracing::warn!(error = %e, "line_chart: failed to create line brush"),
            },
            Err(e) => tracing::warn!(error = %e, "line_chart: failed to build line path"),
        }
    }
}

fn draw_backdrop(draw: &DrawContext<'_>, width: f32, height: f32, background: ColorF) {
    let rect = Rect::from_xywh(0.0, 0.0, width, height);
    match draw.create_solid_brush(background) {
        Ok(brush) => draw.fill_rect(&rect, &brush),
        Err(e) => tracing::warn!(error = %e, "line_chart: failed to create background brush"),
    }
}

fn draw_grid(draw: &DrawContext<'_>, brush: &Brush, width: f32, height: f32) {
    let cols = (width / GRID_TARGET_SPACING_X).ceil().max(1.0) as u32;
    let rows = (height / GRID_TARGET_SPACING_Y).ceil().max(1.0) as u32;
    for row in 1..rows {
        let y = height * (row as f32 / rows as f32);
        draw.draw_line(Vector2 { x: 0.0, y }, Vector2 { x: width, y }, brush, GRID_LINE_WIDTH);
    }
    for col in 1..cols {
        let x = width * (col as f32 / cols as f32);
        draw.draw_line(Vector2 { x, y: 0.0 }, Vector2 { x, y: height }, brush, GRID_LINE_WIDTH);
    }
}

fn build_path(
    device: &GpuDevice,
    series: &Series,
    to_screen: &impl Fn(u64, f32) -> Vector2,
    height: f32,
    filled: bool,
) -> CanvasResult<Path> {
    let screen_points: Vec<Vector2> = series.points.iter().map(|&(t, v)| to_screen(t, v)).collect();
    let first = screen_points[0];

    let builder = PathBuilder::new(device)?;
    let mut figure = if filled {
        builder.begin(Vector2 { x: first.x, y: height }).line_to(first)
    } else {
        builder.begin_hollow(first)
    };

    let last_idx = screen_points.len() - 1;
    for i in 0..last_idx {
        let prev = screen_points[i];
        let next = screen_points[i + 1];
        figure = match series.interpolation {
            Interpolation::Linear => figure.line_to(next),
            Interpolation::Step => figure.line_to(Vector2 { x: next.x, y: prev.y }).line_to(next),
            Interpolation::Smooth => {
                // Catmull-Rom through (before, prev, next, after), converted to a cubic
                // bezier - control points are derived from the *neighboring* points so
                // the tangent is continuous across segment boundaries. A per-segment S-curve
                // (fixed control points at this segment's own midpoint) looks smooth in
                // isolation but has a slope discontinuity at every sample, which reads as
                // jagged/zigzaggy once points are dense relative to how fast the series moves.
                let before = if i == 0 { prev } else { screen_points[i - 1] };
                let after = if i + 1 == last_idx { next } else { screen_points[i + 2] };
                let c1 = Vector2 { x: prev.x + (next.x - before.x) / 6.0, y: prev.y + (next.y - before.y) / 6.0 };
                let c2 = Vector2 { x: next.x - (after.x - prev.x) / 6.0, y: next.y - (after.y - prev.y) / 6.0 };
                figure.bezier_to(c1, c2, next)
            }
        };
    }

    if filled {
        let last = *screen_points.last().expect("checked len >= 2 above");
        figure.line_to(Vector2 { x: last.x, y: height }).close().build()
    } else {
        figure.end_open().build()
    }
}
