use crate::DrawResult;
use plotters::coord::ranged3d::ProjectionMatrix;
use plotters::prelude::*;
use plotters_canvas::CanvasBackend;
use std::collections::BTreeMap;
use web_sys::HtmlCanvasElement;

lazy_static::lazy_static! {
    static ref CTX: Ctx = {
        Ctx::new()
    };
}

pub fn gen_pyramid_surface() -> vdrm_alg::PixelSurface {
    let mut pixel_surface = vdrm_alg::PixelSurface::new();
    for x in 0..64_u32 {
        for y in 0..64_u32 {
            let x_i32 = x as i32 - 32;
            let y_i32 = y as i32 - 32;
            let h = 32 - (x_i32.abs() + y_i32.abs());
            if h < 0 {
                continue;
            }
            let z = h.abs() as u32;
            let color = match (x_i32 >= 0, y_i32 >= 0) {
                (true, true) => 0b111,
                (false, true) => 0b001,
                (false, false) => 0b010,
                (true, false) => 0b101,
            };
            pixel_surface.push((x, y, (z, color)));
        }
    }
    pixel_surface
}
struct Mirror {
    points: [(f32, f32, f32); 4],
}
impl Mirror {
    fn new(len: f32, angle: u32) -> Self {
        let angle = vdrm_alg::angle_to_v(angle);
        let mat = glam::Mat2::from_angle(angle);
        let points = [
            (len, len, -len),
            (-len, len, len),
            (-len, -len, len),
            (len, -len, -len),
        ];
        let points = points.map(|(x, y, z)| {
            let p = mat * glam::Vec2::new(x, y);
            (p.x, p.y, z)
        });
        Self { points }
    }
    fn polygon(&self) -> Polygon<(f32, f32, f32)> {
        Polygon::new(self.points, BLACK.mix(0.2))
    }
}

struct Screen {
    points: [(f32, f32, f32); 4],
}

impl Screen {
    fn new(idx: usize) -> Self {
        let xy_line = vdrm_alg::screens()[idx].xy_line;
        let (a, b) = xy_line.points();
        let points = [
            (a.x(), a.y(), -1.),
            (a.x(), a.y(), 1.),
            (b.x(), b.y(), 1.),
            (b.x(), b.y(), -1.),
        ];
        Self { points }
    }

    fn polygon(&self) -> Polygon<(f32, f32, f32)> {
        Polygon::new(self.points, BLACK.mix(0.8))
    }
}

struct AngleCtx {
    mirror: Mirror,
    led_pixels: Vec<(f32, f32, f32)>,
    emu_pixels: Vec<(f32, f32, f32)>,
}
struct Ctx {
    angle_ctx_map: BTreeMap<u32, AngleCtx>,
    all_real_pixels: Vec<(f32, f32, f32)>,
    all_emu_pixels: Vec<(f32, f32, f32)>,
    all_led_pixels: Vec<(f32, f32, f32)>,
    screens: [Screen; 3],
}

impl Ctx {
    fn new() -> Self {
        let codec = vdrm_alg::Codec::new();
        let pixel_surface = gen_pyramid_surface();
        let all_real_pixels = vdrm_alg::pixel_surface_to_float(&pixel_surface)
            .into_iter()
            .map(|(x, y, z)| (x, y, z - 2.))
            .collect();
        let angle_map = codec.encode(&pixel_surface, 0);
        let (mut all_emu_pixels, mut all_led_pixels) = (vec![], vec![]);
        let angle_ctx_map = (0..vdrm_alg::TOTAL_ANGLES as u32)
            .map(|angle| {
                let mirror = Mirror::new(1. / 2_f32.sqrt(), angle);
                let Some(lines) = angle_map.get(&angle) else {
                    return (
                        angle,
                        AngleCtx {
                            mirror,
                            led_pixels: vec![],
                            emu_pixels: vec![],
                        },
                    );
                };
                let (emu_pixels, led_pixels) = codec.decode(angle, lines);
                all_emu_pixels.extend(emu_pixels.clone());
                all_led_pixels.extend(led_pixels.clone());
                let angle_ctx = AngleCtx {
                    mirror,
                    led_pixels,
                    emu_pixels,
                };
                (angle, angle_ctx)
            })
            .collect();

        Self {
            angle_ctx_map,
            all_real_pixels,
            all_emu_pixels,
            all_led_pixels,
            screens: [0, 1, 2].map(|idx| Screen::new(idx)),
        }
    }
}

pub fn draw(canvas: HtmlCanvasElement, angle: Option<u32>, pitch: f64, yaw: f64) -> DrawResult<()> {
    let area = CanvasBackend::with_canvas_object(canvas)
        .unwrap()
        .into_drawing_area();
    area.fill(&WHITE)?;

    let axis_len = 1.5_f32;
    let x_axis = (-axis_len..axis_len).step(0.1);
    let y_axis = (-axis_len..axis_len).step(0.1);

    let mut chart = ChartBuilder::on(&area).build_cartesian_3d(
        x_axis.clone(),
        y_axis.clone(),
        -axis_len..axis_len,
    )?;
    chart.with_projection(| _pb| {
        let (x, y) = area.get_pixel_range();
        let v = (x.end - x.start).min(y.end - y.start) * 4 / 5 / 2;
        let before = (v, v, v);
        let after = ((x.start + x.end) / 2, (y.start + y.end) / 2);

        let mut mat = if before == (0, 0, 0) {
            ProjectionMatrix::default()
        } else {
            let (x, y, z) = before;
            ProjectionMatrix::shift(-x as f64, -y as f64, -z as f64) * ProjectionMatrix::default()
        };
        if yaw.abs() > 1e-20 {
            mat = mat * ProjectionMatrix::rotate(0.0, 0.0, yaw);
        }
        if pitch.abs() > 1e-20 {
            mat = mat * ProjectionMatrix::rotate(pitch, 0.0, 0.0);
        }
        mat = mat * ProjectionMatrix::scale(0.7);
        if after != (0, 0) {
            let (x, y) = after;
            mat = mat * ProjectionMatrix::shift(x as f64, y as f64, 0.0);
        }
        mat
    });

    chart.configure_axes().draw()?;

    chart
        .draw_series(
            [
                ("x", (axis_len, -axis_len, -axis_len), &RED),
                ("y", (-axis_len, axis_len, -axis_len), &GREEN),
                ("z", (-axis_len, -axis_len, axis_len), &BLUE),
            ]
            .map(|(label, position, color)| {
                Text::new(
                    label,
                    position,
                    ("sans-serif", 20, color).into_text_style(&area),
                )
            }),
        )
        .unwrap();
    let screen_polygons = CTX.screens.iter().map(|v| v.polygon());
    chart
        .draw_series(screen_polygons)?
        .label("SCREEN")
        .legend(|(x, y)| {
            Rectangle::new([(x + 5, y - 5), (x + 15, y + 5)], BLACK.mix(0.9).filled())
        });
    let real_surface_points: PointSeries<_, _, Circle<_, _>, _> =
        PointSeries::new(CTX.all_real_pixels.clone(), 1_f64, &BLUE.mix(0.2));
    chart
        .draw_series(real_surface_points)?
        .label("REAL")
        .legend(|(x, y)| Rectangle::new([(x + 5, y - 5), (x + 15, y + 5)], BLUE.mix(0.5).filled()));

    let (emu, led) = match angle {
        None => {
            (CTX.all_emu_pixels.clone(), CTX.all_led_pixels.clone())
        }
        Some(angle) => {
            let angle_ctx = CTX.angle_ctx_map.get(&angle).unwrap();
            chart
                .draw_series([angle_ctx.mirror.polygon()])?
                .label("MIRROR")
                .legend(|(x, y)| {
                    Rectangle::new([(x + 5, y - 5), (x + 15, y + 5)], BLACK.mix(0.5).filled())
                });

            (angle_ctx.emu_pixels.clone(), angle_ctx.led_pixels.clone())
        }
    };

    let emu_surface_points: PointSeries<_, _, Circle<_, _>, _> =
        PointSeries::new(emu, 1_f64, &RED.mix(0.3));
    chart
        .draw_series(emu_surface_points)?
        .label("EMULATOR")
        .legend(|(x, y)| Rectangle::new([(x + 5, y - 5), (x + 15, y + 5)], RED.mix(0.5).filled()));

    let led_surface_points: PointSeries<_, _, Circle<_, _>, _> =
        PointSeries::new(led, 1_f64, &RED.mix(0.8));
    chart.draw_series(led_surface_points)?;

    chart.configure_series_labels().border_style(BLACK).draw()?;
    Ok(())
}