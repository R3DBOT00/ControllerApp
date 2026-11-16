//! Compact controller-operated overlay. All coordinates are logical pixels.
use crate::{
    assets,
    layout::{Action, Keycap, Layout},
    model::View,
    theme::{self, Metrics},
};
use gtk::cairo::{Context, Filter, LinearGradient};

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
pub fn key_rects(width: f64, height: f64, layout: &Layout) -> Vec<Vec<Rect>> {
    let m = Metrics::new(width, height);
    let available_w = width - 2. * m.inset;
    let cell_h = (m.grid_bottom - m.grid_top) / layout.len() as f64;
    let gap = 4. * m.scale;
    layout
        .iter()
        .enumerate()
        .map(|(r, row)| {
            let total: f64 = row.iter().map(|k| k.units).sum();
            let unit = (available_w - gap * (row.len() - 1) as f64) / total;
            let mut x = m.inset;
            row.iter()
                .map(|key| {
                    let w = key.units * unit;
                    let rect = Rect {
                        x,
                        y: m.grid_top + r as f64 * cell_h,
                        w,
                        h: cell_h - gap,
                    };
                    x += w + gap;
                    rect
                })
                .collect()
        })
        .collect()
}

pub struct Animation {
    pub opacity: f64,
    pub slide: f64,
    pub pulse: f64,
    pub public_assets: bool,
    cursor: Option<Rect>,
    last_pulse: u64,
    blend: f64,
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            opacity: 0.,
            slide: 18.,
            pulse: 0.,
            public_assets: false,
            cursor: None,
            last_pulse: 0,
            blend: 1.,
        }
    }
}
impl Animation {
    pub fn advance(&mut self, view: &View, dt: f64) {
        let step = (dt / 0.016).clamp(0., 4.);
        self.opacity =
            (self.opacity + if view.visible { 0.13 } else { -0.16 } * step).clamp(0., 1.);
        self.slide +=
            ((if view.visible { 0. } else { 18. }) - self.slide) * (1. - 0.72_f64.powf(step));
        self.blend = 1. - 0.65_f64.powf(step);
        if view.pulse != self.last_pulse {
            self.pulse = 1.;
            self.last_pulse = view.pulse;
        }
        self.pulse *= 0.76_f64.powf(step);
        if self.opacity == 0. {
            self.cursor = None;
        }
    }
    pub fn settled_visible() -> Self {
        Self {
            opacity: 1.,
            slide: 0.,
            ..Self::default()
        }
    }
}
fn rgb(ctx: &Context, color: [u8; 3]) {
    rgba(ctx, color, 1.);
}
fn rgba(ctx: &Context, color: [u8; 3], alpha: f64) {
    ctx.set_source_rgba(
        f64::from(color[0]) / 255.,
        f64::from(color[1]) / 255.,
        f64::from(color[2]) / 255.,
        alpha,
    );
}
fn round(ctx: &Context, r: Rect, radius: f64) {
    let radius = radius.min(r.w / 2.).min(r.h / 2.).max(0.);
    let p = std::f64::consts::FRAC_PI_2;
    ctx.new_sub_path();
    ctx.arc(r.x + r.w - radius, r.y + radius, radius, -p, 0.);
    ctx.arc(r.x + r.w - radius, r.y + r.h - radius, radius, 0., p);
    ctx.arc(r.x + radius, r.y + r.h - radius, radius, p, 2. * p);
    ctx.arc(r.x + radius, r.y + radius, radius, 2. * p, 3. * p);
    ctx.close_path();
}
fn box_fill(ctx: &Context, r: Rect, fill: [u8; 3], radius: f64) {
    round(ctx, r, radius);
    rgb(ctx, fill);
    let _ = ctx.fill();
}
fn outline(ctx: &Context, r: Rect, radius: f64, color: [u8; 3], alpha: f64, width: f64) {
    round(ctx, r, radius);
    rgba(ctx, color, alpha);
    ctx.set_line_width(width);
    let _ = ctx.stroke();
}
fn label(ctx: &Context, text: &str, r: Rect, size: f64, color: [u8; 3], center: bool, bold: bool) {
    if r.w <= 0. || r.h <= 0. {
        return;
    }
    let _ = ctx.save();
    ctx.rectangle(r.x, r.y, r.w, r.h);
    ctx.clip();
    let layout = pangocairo::functions::create_layout(ctx);
    let mut font =
        gtk::pango::FontDescription::from_string(if bold { "Sans Semi-Bold" } else { "Sans" });
    font.set_absolute_size(size * f64::from(gtk::pango::SCALE));
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    let (ink, _) = layout.pixel_extents();
    if center && f64::from(ink.width()) > r.w - 4. {
        font.set_absolute_size(
            size * ((r.w - 4.) / f64::from(ink.width())).clamp(0.45, 1.)
                * f64::from(gtk::pango::SCALE),
        );
        layout.set_font_description(Some(&font));
    } else if !center {
        layout.set_width((r.w * f64::from(gtk::pango::SCALE)) as i32);
        layout.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    }
    let (ext, _) = layout.pixel_extents();
    let x = if center {
        r.x + (r.w - f64::from(ext.width())) / 2. - f64::from(ext.x())
    } else {
        r.x
    };
    ctx.move_to(
        x,
        r.y + (r.h - f64::from(ext.height())) / 2. - f64::from(ext.y()),
    );
    rgb(ctx, color);
    pangocairo::functions::show_layout(ctx, &layout);
    let _ = ctx.restore();
}
fn key_label(ctx: &Context, key: &Keycap, r: Rect, shift: bool, s: f64) {
    let special = !matches!(key.action, Action::Character(..));
    label(
        ctx,
        key.text(shift),
        r,
        if special { 12. * s } else { 19. * s },
        theme::TEXT,
        true,
        special,
    );
    if let Action::Character(a, b) = key.action
        && !a.is_alphabetic()
        && a != b
    {
        let secondary = if shift { &key.label } else { &key.shifted };
        label(
            ctx,
            secondary,
            Rect {
                x: r.x + r.w - 13. * s,
                y: r.y + 2. * s,
                w: 11. * s,
                h: 12. * s,
            },
            9. * s,
            theme::MUTED,
            true,
            false,
        );
    }
}
fn chip(ctx: &Context, text: &str, rect: Rect, active: bool, s: f64) {
    box_fill(
        ctx,
        rect,
        if active { [51, 72, 29] } else { [37, 41, 37] },
        4. * s,
    );
    label(
        ctx,
        text,
        rect,
        10. * s,
        if active { theme::GREEN } else { theme::MUTED },
        true,
        true,
    );
}
fn legend(ctx: &Context, width: f64, m: Metrics, public: bool) {
    let s = m.scale;
    let available = width - m.inset * 2.;
    let button_weights = if m.narrow {
        [0.25; 4]
    } else {
        [0.085, 0.085, 0.145, 0.085]
    };
    let mut x = m.inset;
    assets::LOCAL.with(|assets| {
        for (i, (button, action)) in [
            ("A", "Select"),
            ("B", "Close"),
            ("X", "Backspace"),
            ("Y", "Space"),
        ]
        .into_iter()
        .enumerate()
        {
            let slot = available * button_weights[i];
            let mut text_x = x;
            let mut text = format!("{button}  {action}");
            if !public && let Some(images) = &assets.buttons {
                let surface = &images[i];
                let scale = 24. * s / f64::from(surface.height());
                let _ = ctx.save();
                ctx.translate(x, m.footer_top);
                ctx.scale(scale, scale);
                if ctx.set_source_surface(surface, 0., 0.).is_ok() {
                    ctx.source().set_filter(Filter::Best);
                    let _ = ctx.paint();
                }
                let _ = ctx.restore();
                text_x += f64::from(surface.width()) * scale + 2. * s;
                text = action.into();
            }
            label(
                ctx,
                &text,
                Rect {
                    x: text_x,
                    y: m.footer_top,
                    w: slot - (text_x - x) - 4. * s,
                    h: 24. * s,
                },
                10.5 * s,
                theme::MUTED,
                false,
                false,
            );
            x += slot;
        }
    });
    let (mut x, y, weights) = if m.narrow {
        (m.inset, m.footer_top + 25. * s, [0.36, 0.24, 0.4])
    } else {
        (x, m.footer_top, [0.22, 0.14, 0.24])
    };
    for (i, text) in ["LB / RB / R3  Shift", "Start  Enter", "Start + Back  Mode"]
        .into_iter()
        .enumerate()
    {
        let w = available * weights[i];
        label(
            ctx,
            text,
            Rect {
                x,
                y,
                w: w - 4. * s,
                h: 24. * s,
            },
            10.5 * s,
            theme::MUTED,
            true,
            false,
        );
        x += w;
    }
}
pub fn draw(
    ctx: &Context,
    width: f64,
    height: f64,
    view: &View,
    layout: &Layout,
    animation: &mut Animation,
) {
    ctx.set_operator(gtk::cairo::Operator::Source);
    ctx.set_source_rgba(0., 0., 0., 0.);
    let _ = ctx.paint();
    ctx.set_operator(gtk::cairo::Operator::Over);
    if animation.opacity <= 0. || width < 1. || height < 1. {
        return;
    }
    let m = Metrics::new(width, height);
    let s = m.scale;
    let _ = ctx.save();
    ctx.translate(0., animation.slide * s);
    ctx.push_group();
    let panel = Rect {
        x: 6. * s,
        y: 6. * s,
        w: width - 12. * s,
        h: height - 12. * s,
    };
    for spread in [5., 3., 1.] {
        round(
            ctx,
            Rect {
                x: panel.x - spread * s,
                y: panel.y + 2. * s,
                w: panel.w + 2. * spread * s,
                h: panel.h + spread * s,
            },
            (12. + spread) * s,
        );
        ctx.set_source_rgba(0., 0., 0., 0.12);
        let _ = ctx.fill();
    }
    box_fill(ctx, panel, theme::PANEL, 12. * s);
    outline(ctx, panel, 12. * s, theme::BORDER, 0.8, s);
    box_fill(
        ctx,
        Rect {
            x: m.inset,
            y: 18. * s,
            w: 3. * s,
            h: 12. * s,
        },
        theme::GREEN,
        1.5 * s,
    );
    label(
        ctx,
        "ControllerApp",
        Rect {
            x: m.inset + 11. * s,
            y: 13. * s,
            w: 130. * s,
            h: 22. * s,
        },
        13. * s,
        theme::TEXT,
        false,
        true,
    );
    let caps = match view.caps_lock {
        Some(true) => "SYS CAPS ON",
        Some(false) => "SYS CAPS OFF",
        None => "SYS CAPS —",
    };
    let chips = [
        (if view.function { "FN" } else { "PT-BR" }, false, 52.),
        (
            if view.shift { "SHIFT ON" } else { "SHIFT OFF" },
            view.shift,
            78.,
        ),
        (caps, view.caps_lock == Some(true), 99.),
    ];
    let mut x = width - m.inset - 241. * s;
    for (text, active, w) in chips {
        chip(
            ctx,
            text,
            Rect {
                x,
                y: 14. * s,
                w: w * s,
                h: 20. * s,
            },
            active,
            s,
        );
        x += (w + 6.) * s;
    }
    let preview = Rect {
        x: m.inset,
        y: 42. * s,
        w: width - 2. * m.inset,
        h: 27. * s,
    };
    box_fill(ctx, preview, [19, 22, 19], 5. * s);
    outline(ctx, preview, 5. * s, theme::BORDER, 0.65, s);
    let text = if let Some(error) = &view.error {
        error.clone()
    } else if view.protected {
        "Protected field · preview hidden".into()
    } else if view.preview.is_empty() {
        "Type here…".into()
    } else {
        format!("{}│", view.preview)
    };
    label(
        ctx,
        &text,
        Rect {
            x: preview.x + 10. * s,
            w: preview.w - 20. * s,
            ..preview
        },
        14. * s,
        if view.preview.is_empty() || view.protected {
            theme::MUTED
        } else {
            theme::TEXT
        },
        false,
        false,
    );
    let rects = key_rects(width, height, layout);
    for (r, row) in layout.iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            let rect = rects[r][c];
            box_fill(
                ctx,
                Rect {
                    y: rect.y + s,
                    ..rect
                },
                [14, 16, 14],
                5. * s,
            );
            box_fill(
                ctx,
                rect,
                if matches!(key.action, Action::Character(..)) {
                    theme::KEY
                } else {
                    theme::SPECIAL
                },
                5. * s,
            );
            outline(ctx, rect, 5. * s, theme::BORDER, 0.5, 0.8 * s);
        }
    }
    let row = view.row.min(layout.len() - 1);
    let col = view.col.min(layout[row].len() - 1);
    let target = rects[row][col];
    let current = animation.cursor.get_or_insert(target);
    current.x += (target.x - current.x) * animation.blend;
    current.y += (target.y - current.y) * animation.blend;
    current.w += (target.w - current.w) * animation.blend;
    current.h += (target.h - current.h) * animation.blend;
    let selected = *current;
    for (spread, alpha) in [(4., 0.05), (2.5, 0.09), (1., 0.16)] {
        outline(
            ctx,
            Rect {
                x: selected.x - spread * s,
                y: selected.y - spread * s,
                w: selected.w + 2. * spread * s,
                h: selected.h + 2. * spread * s,
            },
            (5. + spread) * s,
            theme::GREEN,
            alpha,
            2. * s,
        );
    }
    round(ctx, selected, 5. * s);
    let gradient = LinearGradient::new(0., selected.y, 0., selected.y + selected.h);
    gradient.add_color_stop_rgb(
        0.,
        0.40 + animation.pulse * 0.12,
        0.62 + animation.pulse * 0.12,
        0.10,
    );
    gradient.add_color_stop_rgb(1., 0.23, 0.40, 0.07);
    let _ = ctx.set_source(&gradient);
    let _ = ctx.fill();
    outline(ctx, selected, 5. * s, theme::GREEN, 0.9, 1.2 * s);
    for (r, row) in layout.iter().enumerate() {
        for (c, key) in row.iter().enumerate() {
            key_label(ctx, key, rects[r][c], view.shift, s);
        }
    }
    legend(ctx, width, m, animation.public_assets);
    let _ = ctx.pop_group_to_source();
    let _ = ctx.paint_with_alpha(animation.opacity);
    let _ = ctx.restore();
}

pub fn render_png(
    path: &std::path::Path,
    width: i32,
    height: i32,
    public: bool,
    function: bool,
    shift: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, width, height)?;
    let ctx = Context::new(&surface)?;
    let view = View {
        visible: true,
        function,
        shift,
        ..View::default()
    };
    let mut animation = Animation::settled_visible();
    animation.public_assets = public;
    let layout = if function {
        crate::layout::function_layout()
    } else {
        crate::layout::main_layout()
    };
    draw(
        &ctx,
        f64::from(width),
        f64::from(height),
        &view,
        &layout,
        &mut animation,
    );
    let mut file = std::fs::File::create(path)?;
    surface.write_to_png(&mut file)?;
    Ok(())
}
