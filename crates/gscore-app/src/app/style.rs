//! The app's look: one small set of design tokens per theme (surfaces in layers, one accent
//! color, text and line colors), spacing on a 4-pixel grid and the type scale, applied to
//! egui's light and dark styles once; plus the shared building blocks of the shell (panel
//! title bars, titled sections, toolbar groups).
//!
//! The ideas follow MuseScore 4's calm, layered design; the colors are GuitarScore's own.
//! The page's own colors come from `gscore-layout`'s palette.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Frame, Margin, RichText, Shadow, Stroke, TextStyle,
    Theme, vec2,
};

use crate::fonts::BOLD;

/// The colors of one theme.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tokens {
    /// Menu bar, toolbar and status bar: the outermost layer.
    pub chrome: Color32,
    /// Docked panels.
    pub panel: Color32,
    /// The canvas the page lies on.
    pub canvas: Color32,
    /// Controls at rest (buttons, fields), hovered, and pressed.
    pub control: Color32,
    pub control_hover: Color32,
    pub control_active: Color32,
    /// Lines between areas and around controls.
    pub line: Color32,
    /// Text, and secondary text (labels, hints).
    pub text: Color32,
    pub text_weak: Color32,
    /// The one accent: chosen items, focus, sliders.
    pub accent: Color32,
    /// Text on the accent.
    pub on_accent: Color32,
    /// Mute (red) and solo (amber): the two mixer states that must never be confused,
    /// neither with each other nor with the blue accent.
    pub mute: Color32,
    pub solo: Color32,
}

/// The app's tokens. The chrome is always dark (a soft charcoal, white icons and text),
/// whatever the score's theme: only the page follows the light or dark choice.
pub(crate) const TOKENS: Tokens = Tokens {
    chrome: Color32::from_rgb(0x26, 0x28, 0x2C),
    panel: Color32::from_rgb(0x2E, 0x30, 0x35),
    canvas: Color32::from_rgb(0x46, 0x49, 0x4F),
    control: Color32::from_rgb(0x3A, 0x3D, 0x43),
    control_hover: Color32::from_rgb(0x46, 0x4A, 0x51),
    control_active: Color32::from_rgb(0x52, 0x56, 0x5E),
    line: Color32::from_rgb(0x3B, 0x3E, 0x44),
    text: Color32::from_rgb(0xEE, 0xEF, 0xF1),
    text_weak: Color32::from_rgb(0xA6, 0xAA, 0xB1),
    accent: Color32::from_rgb(0x4C, 0x9A, 0xFF),
    on_accent: Color32::WHITE,
    mute: Color32::from_rgb(0xE0, 0x4A, 0x45),
    solo: Color32::from_rgb(0xE9, 0xA8, 0x2E),
};

/// The red behind a window's or tab's close button while it is hovered.
pub(crate) const CLOSE_RED: Color32 = Color32::from_rgb(0xD9, 0x3A, 0x3A);

/// The app's tokens (the same in every theme; `ui` keeps call sites uniform).
pub(crate) fn tokens(_ui: &egui::Ui) -> &'static Tokens {
    &TOKENS
}

/// The app's tokens, where only a context is at hand.
pub(crate) fn context_tokens(_ctx: &egui::Context) -> &'static Tokens {
    &TOKENS
}

/// Rounding of controls and of the page shadow.
const RADIUS: u8 = 4;

/// Applies GuitarScore's tokens, spacing and typography and switches egui to its dark
/// theme for good (the chrome does not follow the score's light or dark choice). Both of
/// egui's styles get the same look, so nothing changes if the system theme flips.
pub(crate) fn install(ctx: &egui::Context) {
    for theme in [Theme::Light, Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = egui::Visuals::dark();
            apply_tokens(&mut style.visuals, &TOKENS);
            apply_metrics(style);
        });
    }
    ctx.set_theme(Theme::Dark);
}

fn apply_tokens(visuals: &mut egui::Visuals, t: &Tokens) {
    let radius = CornerRadius::same(RADIUS);
    visuals.panel_fill = t.panel;
    visuals.window_fill = t.panel;
    visuals.extreme_bg_color = Color32::from_rgb(0x22, 0x24, 0x27);
    visuals.faint_bg_color = t.control.gamma_multiply(0.45);
    visuals.code_bg_color = t.control;
    visuals.window_stroke = Stroke::new(1.0, t.line);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    visuals.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(100),
    };
    visuals.selection.bg_fill = t.accent;
    visuals.selection.stroke = Stroke::new(1.0, t.on_accent);
    visuals.hyperlink_color = t.accent;
    visuals.override_text_color = None;

    let widgets = &mut visuals.widgets;
    // Labels, separators and frames that do not react.
    widgets.noninteractive.bg_fill = t.panel;
    widgets.noninteractive.weak_bg_fill = t.panel;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.line);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    widgets.noninteractive.corner_radius = radius;
    // Controls at rest: filled, no outline (a calm surface), text in the normal color.
    widgets.inactive.bg_fill = t.control;
    widgets.inactive.weak_bg_fill = t.control;
    widgets.inactive.bg_stroke = Stroke::NONE;
    widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    widgets.inactive.corner_radius = radius;
    widgets.inactive.expansion = 0.0;
    widgets.hovered.bg_fill = t.control_hover;
    widgets.hovered.weak_bg_fill = t.control_hover;
    widgets.hovered.bg_stroke = Stroke::NONE;
    widgets.hovered.fg_stroke = Stroke::new(1.5, t.text);
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;
    widgets.active.bg_fill = t.control_active;
    widgets.active.weak_bg_fill = t.control_active;
    widgets.active.bg_stroke = Stroke::new(1.0, t.accent);
    widgets.active.fg_stroke = Stroke::new(2.0, t.text);
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;
    widgets.open.bg_fill = t.control_hover;
    widgets.open.weak_bg_fill = t.control_hover;
    widgets.open.bg_stroke = Stroke::new(1.0, t.line);
    widgets.open.fg_stroke = Stroke::new(1.0, t.text);
    widgets.open.corner_radius = radius;
}

fn apply_metrics(style: &mut egui::Style) {
    let spacing = &mut style.spacing;
    spacing.item_spacing = vec2(8.0, 6.0);
    spacing.button_padding = vec2(8.0, 4.0);
    spacing.menu_margin = Margin::same(6);
    spacing.window_margin = Margin::same(12);
    spacing.indent = 12.0;
    spacing.interact_size = vec2(24.0, 24.0);
    spacing.slider_rail_height = 4.0;
    spacing.combo_width = 120.0;
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(15.0, FontFamily::Name(BOLD.into())),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(12.0, FontFamily::Monospace),
    );
}

/// The frame of the menu bar, toolbar and status bar (the outer chrome layer).
pub(crate) fn chrome_frame(ctx: &egui::Context, vertical: i8) -> Frame {
    let tokens = context_tokens(ctx);
    Frame::new()
        .fill(tokens.chrome)
        .inner_margin(Margin::symmetric(8, vertical))
        .stroke(Stroke::NONE)
}

/// The frame of a docked panel: its fill and a thin line towards the canvas.
pub(crate) fn panel_frame(ctx: &egui::Context) -> Frame {
    let tokens = context_tokens(ctx);
    Frame::new()
        .fill(tokens.panel)
        .inner_margin(Margin::same(0))
        .stroke(Stroke::new(1.0, tokens.line))
}

/// The canvas behind the page.
pub(crate) fn canvas_frame(ctx: &egui::Context) -> Frame {
    let tokens = context_tokens(ctx);
    Frame::new()
        .fill(tokens.canvas)
        .inner_margin(Margin::same(0))
}

/// The frame of one of the app's own (floating) windows: panel fill, a thin edge and no
/// inner margin, so the title bar spans the full width.
pub(crate) fn window_frame(ctx: &egui::Context) -> Frame {
    let tokens = context_tokens(ctx);
    Frame::new()
        .fill(tokens.panel)
        .stroke(Stroke::new(1.0, tokens.line))
        .inner_margin(Margin::same(0))
}

/// Where a floating window of `size` starts: centered in the app.
pub(crate) fn centered_window_pos(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
    let screen = ctx.content_rect();
    egui::pos2(
        screen.center().x - size.x / 2.0,
        screen.center().y - size.y / 2.0,
    )
}

/// The title bar of one of the app's own windows, styled like the main title bar (same
/// height and chrome color). Dragging it moves the window via `pos`; returns whether the
/// close button was clicked.
pub(crate) fn window_title_bar(
    ui: &mut egui::Ui,
    title: &str,
    pos: &mut Option<egui::Pos2>,
) -> bool {
    let tokens = context_tokens(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, super::title_bar::TITLE_BAR_HEIGHT),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, tokens.chrome);
    let close_rect =
        egui::Rect::from_min_max(egui::pos2(rect.right() - 34.0, rect.top()), rect.max);
    let title_rect =
        egui::Rect::from_min_max(rect.min, egui::pos2(close_rect.left(), rect.bottom()));
    painter.text(
        title_rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        FontId::new(13.0, FontFamily::Name(BOLD.into())),
        tokens.text,
    );
    if response.dragged_by(egui::PointerButton::Primary)
        && let Some(position) = pos
    {
        *position += response.drag_delta();
        ui.ctx().request_repaint();
    }
    let close = ui.interact(
        close_rect,
        ui.scope_id().with("window_close"),
        egui::Sense::click(),
    );
    ui.ctx().accesskit_node_builder(close.id, |node| {
        node.set_label(format!("Close {title}"));
        node.set_role(egui::Role::Button);
    });
    if close.hovered() {
        painter.rect_filled(
            close_rect,
            0.0,
            Color32::from_rgb(0xD9, 0x3A, 0x3A).gamma_multiply(0.65),
        );
    }
    let ink = if close.hovered() {
        Color32::WHITE
    } else {
        tokens.text_weak
    };
    let cross = Stroke::new(1.3, ink);
    let cross_rect = close_rect.shrink(12.0);
    painter.line_segment([cross_rect.left_top(), cross_rect.right_bottom()], cross);
    painter.line_segment([cross_rect.right_top(), cross_rect.left_bottom()], cross);
    close.clicked()
}

/// A titled section of a panel that can be folded away; its content is indented by the
/// panel's margin. The title is small, bold and muted, like a property-sheet group.
pub(crate) fn section<R>(
    ui: &mut egui::Ui,
    id: &str,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<R> {
    let tokens = tokens(ui);
    let heading = RichText::new(title.to_uppercase())
        .font(FontId::new(11.0, FontFamily::Name(BOLD.into())))
        .color(tokens.text_weak);
    let result = Frame::new()
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 4,
            bottom: 8,
        })
        .show(ui, |ui| {
            egui::CollapsingHeader::new(heading)
                .id_salt(id)
                .default_open(true)
                .show_unindented(ui, |ui| {
                    ui.add_space(2.0);
                    add_contents(ui)
                })
                .body_returned
        })
        .inner;
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, tokens.line),
    );
    result
}

/// A rounded group of toolbar controls on a slightly raised surface.
pub(crate) fn toolbar_group<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let tokens = tokens(ui);
    Frame::new()
        .fill(tokens.panel)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add_contents).inner
        })
        .inner
}

/// The body of one of the app's own windows: the shared inner frame and margins. Returns
/// whether Escape was pressed, which closes every dialog; the caller treats that exactly
/// like its close button.
pub(crate) fn dialog_body(
    ui: &mut egui::Ui,
    margin: Margin,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut escape = false;
    Frame::new().inner_margin(margin).show(ui, |ui| {
        add_contents(ui);
        escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
    });
    escape
}
