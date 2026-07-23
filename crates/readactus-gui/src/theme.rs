//! Daemon Labs brand theme for Readactus.
//!
//! Centralises the whole look & feel: the brand palette, the three brand
//! typefaces (Bricolage Grotesque, Hanken Grotesk, JetBrains Mono), a hand
//! tuned light and dark egui `Visuals`, and a small set of layout/widget
//! helpers so every screen shares the same spacing, cards and buttons.
//!
//! To retheme the app, change the palette constants below — nothing else in
//! the codebase hard-codes a colour.

use std::sync::Arc;

use eframe::egui;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response,
    RichText, Stroke, TextStyle, Theme, ThemePreference, Vec2, Visuals, WidgetText,
};

// ---------------------------------------------------------------------------
// Brand palette (from the Daemon Labs brand guide)
// ---------------------------------------------------------------------------

/// Warm off-white — the primary light surface.
pub const CANVAS: Color32 = Color32::from_rgb(0xFE, 0xF7, 0xE5);
/// Near-white cream — raised light surfaces (cards, inputs).
pub const PAPER: Color32 = Color32::from_rgb(0xFF, 0xFC, 0xF2);
/// Damian Red — the primary accent (CTAs, selection, focus).
pub const DAMIAN_RED: Color32 = Color32::from_rgb(0xDF, 0x4D, 0x39);
/// Deep Ember — pressed/active accent.
pub const DEEP_EMBER: Color32 = Color32::from_rgb(0xA2, 0x42, 0x37);
/// Midnight Ink — the primary dark surface / light-mode text.
pub const MIDNIGHT_INK: Color32 = Color32::from_rgb(0x1E, 0x2F, 0x3D);
/// Circuit Blue — muted secondary (borders, hints).
pub const CIRCUIT_BLUE: Color32 = Color32::from_rgb(0xA6, 0xC3, 0xC3);

// Derived surfaces — kept here so the whole tonal ramp lives in one place.
const INK_RAISED: Color32 = Color32::from_rgb(0x27, 0x3A, 0x4B); // dark card / input
const INK_HOVER: Color32 = Color32::from_rgb(0x30, 0x46, 0x59);
const INK_ACTIVE: Color32 = Color32::from_rgb(0x39, 0x52, 0x67);
const INK_SUNKEN: Color32 = Color32::from_rgb(0x16, 0x23, 0x2E); // dark extreme bg
const BORDER_DARK: Color32 = Color32::from_rgb(0x35, 0x4A, 0x5C);

const CANVAS_HOVER: Color32 = Color32::from_rgb(0xF4, 0xEC, 0xD6);
const CANVAS_ACTIVE: Color32 = Color32::from_rgb(0xEA, 0xE1, 0xC7);
const BORDER_LIGHT: Color32 = Color32::from_rgb(0xE1, 0xD6, 0xBC);
const CANVAS_MUTED_TEXT: Color32 = Color32::from_rgb(0x5C, 0x69, 0x73);

/// The custom font family used for headings (Bricolage Grotesque).
fn brand_display() -> FontFamily {
    FontFamily::Name("Bricolage".into())
}

/// Max width of the centred content column, in points.
pub const CONTENT_WIDTH: f32 = 640.0;

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Install fonts, the type scale, spacing and both light/dark palettes, then
/// follow the OS light/dark appearance.
pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    // Type scale + spacing apply to both themes (they live on `Style`, not
    // `Visuals`, so setting per-theme visuals below won't clobber them).
    ctx.all_styles_mut(|style| {
        style.text_styles = text_styles();
        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(10.0, 10.0);
        s.button_padding = Vec2::new(16.0, 9.0);
        s.interact_size.y = 30.0;
        s.menu_margin = Margin::same(6);
        s.window_margin = Margin::same(12);
    });

    ctx.set_visuals_of(Theme::Light, light_visuals());
    ctx.set_visuals_of(Theme::Dark, dark_visuals());

    // Follow the OS appearance, and keep following it if it changes at runtime.
    ctx.set_theme(ThemePreference::System);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "Hanken".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/HankenGrotesk.ttf"
        ))),
    );
    fonts.font_data.insert(
        "Bricolage".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/BricolageGrotesque.ttf"
        ))),
    );
    fonts.font_data.insert(
        "JetBrains".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/JetBrainsMono.ttf"
        ))),
    );

    // Hanken is the primary proportional face; JetBrains the monospace one.
    // We insert at the front so egui's bundled fonts remain as glyph fallbacks.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Hanken".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "JetBrains".to_owned());
    // Bricolage is a dedicated display family for headings, with Hanken as a
    // glyph fallback.
    fonts.families.insert(
        brand_display(),
        vec!["Bricolage".to_owned(), "Hanken".to_owned()],
    );

    ctx.set_fonts(fonts);
}

fn text_styles() -> std::collections::BTreeMap<TextStyle, FontId> {
    use FontFamily::{Monospace, Proportional};
    [
        (TextStyle::Heading, FontId::new(26.0, brand_display())),
        (TextStyle::Body, FontId::new(15.5, Proportional)),
        (TextStyle::Button, FontId::new(15.0, Proportional)),
        (TextStyle::Monospace, FontId::new(14.0, Monospace)),
        (TextStyle::Small, FontId::new(12.5, Proportional)),
    ]
    .into()
}

// ---------------------------------------------------------------------------
// Palettes
// ---------------------------------------------------------------------------

fn widget(bg: Color32, weak: Color32, stroke: Color32, fg: Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: weak,
        bg_stroke: Stroke::new(1.0, stroke),
        corner_radius: CornerRadius::same(7),
        fg_stroke: Stroke::new(1.5, fg),
        expansion: 0.0,
    }
}

fn light_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.override_text_color = Some(MIDNIGHT_INK);
    v.panel_fill = CANVAS;
    v.window_fill = CANVAS;
    v.extreme_bg_color = PAPER;
    v.faint_bg_color = CANVAS_HOVER;
    v.hyperlink_color = DEEP_EMBER;
    v.warn_fg_color = DEEP_EMBER;
    v.error_fg_color = DAMIAN_RED;
    v.window_corner_radius = CornerRadius::same(10);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xDF, 0x4D, 0x39, 60);
    v.selection.stroke = Stroke::new(1.0, DEEP_EMBER);

    v.widgets.noninteractive = widget(CANVAS, CANVAS, BORDER_LIGHT, MIDNIGHT_INK);
    v.widgets.inactive = widget(PAPER, PAPER, BORDER_LIGHT, MIDNIGHT_INK);
    v.widgets.hovered = {
        let mut w = widget(CANVAS_HOVER, CANVAS_HOVER, DAMIAN_RED, MIDNIGHT_INK);
        w.expansion = 1.0;
        w
    };
    v.widgets.active = {
        let mut w = widget(CANVAS_ACTIVE, CANVAS_ACTIVE, DEEP_EMBER, MIDNIGHT_INK);
        w.expansion = 1.0;
        w
    };
    v.widgets.open = widget(CANVAS_HOVER, CANVAS_HOVER, DAMIAN_RED, MIDNIGHT_INK);
    v
}

fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.override_text_color = Some(CANVAS);
    v.panel_fill = MIDNIGHT_INK;
    v.window_fill = MIDNIGHT_INK;
    v.extreme_bg_color = INK_SUNKEN;
    v.faint_bg_color = INK_RAISED;
    v.hyperlink_color = DAMIAN_RED;
    v.warn_fg_color = Color32::from_rgb(0xF0, 0x8A, 0x5C);
    v.error_fg_color = Color32::from_rgb(0xF0, 0x6A, 0x55);
    v.window_corner_radius = CornerRadius::same(10);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xDF, 0x4D, 0x39, 90);
    v.selection.stroke = Stroke::new(1.0, DAMIAN_RED);

    v.widgets.noninteractive = widget(MIDNIGHT_INK, MIDNIGHT_INK, BORDER_DARK, CANVAS);
    v.widgets.inactive = widget(INK_RAISED, INK_RAISED, BORDER_DARK, CANVAS);
    v.widgets.hovered = {
        let mut w = widget(INK_HOVER, INK_HOVER, DAMIAN_RED, CANVAS);
        w.expansion = 1.0;
        w
    };
    v.widgets.active = {
        let mut w = widget(INK_ACTIVE, INK_ACTIVE, DAMIAN_RED, PAPER);
        w.expansion = 1.0;
        w
    };
    v.widgets.open = widget(INK_HOVER, INK_HOVER, DAMIAN_RED, CANVAS);
    v
}

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

/// The brand accent, for callers that want to colour their own text/icons.
pub fn accent() -> Color32 {
    DAMIAN_RED
}

/// Low-emphasis text colour for the current theme (captions, hints).
pub fn muted(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        CIRCUIT_BLUE
    } else {
        CANVAS_MUTED_TEXT
    }
}

/// Raised surface colour for cards on the current theme.
fn surface(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        INK_RAISED
    } else {
        PAPER
    }
}

fn border(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        BORDER_DARK
    } else {
        BORDER_LIGHT
    }
}

// ---------------------------------------------------------------------------
// Layout & widget helpers
// ---------------------------------------------------------------------------

/// Centre the page content in a fixed-width column so nothing stretches
/// edge-to-edge on a wide window. The column itself is left-aligned (natural
/// for forms); use [`hero`]/`vertical_centered` for centred hero content.
pub fn page<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let avail = ui.available_width();
    let w = CONTENT_WIDTH.min(avail);
    let margin = ((avail - w) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(margin);
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.add_space(28.0);
            add(ui)
        })
        .inner
    })
    .inner
}

/// Centre hero content (wordmark, title) horizontally within the column.
pub fn hero<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.vertical_centered(|ui| add(ui)).inner
}

/// The Readactus wordmark: "Readact" in the display face with a red "us",
/// followed by a muted tagline.
pub fn brand_header(ui: &mut egui::Ui, tagline: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let size = 34.0;
        ui.label(
            RichText::new("Readact")
                .family(brand_display())
                .size(size),
        );
        ui.label(
            RichText::new("us")
                .family(brand_display())
                .size(size)
                .color(DAMIAN_RED),
        );
    });
    if !tagline.is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new(tagline).size(14.0).color(muted(ui)));
    }
}

/// A screen title in the display face.
pub fn title(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .family(brand_display())
            .size(24.0),
    );
}

/// Muted caption/subtitle text.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(muted(ui)));
}

/// A raised, padded, rounded surface for grouping related controls.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(surface(ui))
        .stroke(Stroke::new(1.0, border(ui)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A filled, accent-coloured primary action button.
pub fn primary_button(ui: &mut egui::Ui, text: impl Into<String>) -> Response {
    let label = RichText::new(text.into())
        .color(PAPER)
        .strong()
        .size(15.0);
    let resp = ui.add(
        egui::Button::new(label)
            .fill(DAMIAN_RED)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 38.0)),
    );
    // Subtle hover/press feedback on top of the flat fill.
    if resp.hovered() {
        let overlay = if resp.is_pointer_button_down_on() {
            Color32::from_black_alpha(38)
        } else {
            Color32::from_white_alpha(22)
        };
        ui.painter()
            .rect_filled(resp.rect, CornerRadius::same(8), overlay);
    }
    resp
}

/// [`primary_button`], but greyed out and non-interactive when `!enabled`.
pub fn primary_button_enabled(
    ui: &mut egui::Ui,
    text: impl Into<String>,
    enabled: bool,
) -> Response {
    if enabled {
        return primary_button(ui, text);
    }
    let label = RichText::new(text.into()).color(muted(ui)).size(15.0);
    ui.add_enabled(
        false,
        egui::Button::new(label)
            .fill(surface(ui))
            .stroke(Stroke::new(1.0, border(ui)))
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 38.0)),
    )
}

/// A quieter, outlined secondary button. `enabled` mirrors `add_enabled`.
pub fn secondary_button(ui: &mut egui::Ui, text: impl Into<WidgetText>, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .corner_radius(CornerRadius::same(8))
            .min_size(Vec2::new(0.0, 38.0)),
    )
}
