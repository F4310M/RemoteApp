//! Look of the app: colours, sizes and corners taken from the Tailwind CSS
//! palette (slate for surfaces and text, indigo for actions), so the native
//! window looks like a modern web dashboard.

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke};

/// Tailwind colours by name, e.g. `SLATE_200` is `slate-200`.
pub mod tw {
    use eframe::egui::Color32;
    pub const WHITE: Color32 = Color32::WHITE;
    pub const SLATE_50: Color32 = Color32::from_rgb(0xF8, 0xFA, 0xFC);
    pub const SLATE_100: Color32 = Color32::from_rgb(0xF1, 0xF5, 0xF9);
    pub const SLATE_200: Color32 = Color32::from_rgb(0xE2, 0xE8, 0xF0);
    pub const SLATE_300: Color32 = Color32::from_rgb(0xCB, 0xD5, 0xE1);
    pub const SLATE_400: Color32 = Color32::from_rgb(0x94, 0xA3, 0xB8);
    pub const SLATE_500: Color32 = Color32::from_rgb(0x64, 0x74, 0x8B);
    pub const SLATE_700: Color32 = Color32::from_rgb(0x33, 0x41, 0x55);
    pub const SLATE_800: Color32 = Color32::from_rgb(0x1E, 0x29, 0x3B);
    pub const SLATE_900: Color32 = Color32::from_rgb(0x0F, 0x17, 0x2A);
    pub const INDIGO_400: Color32 = Color32::from_rgb(0x81, 0x8C, 0xF8);
    pub const INDIGO_500: Color32 = Color32::from_rgb(0x63, 0x66, 0xF1);
    pub const INDIGO_600: Color32 = Color32::from_rgb(0x4F, 0x46, 0xE5);
    pub const INDIGO_700: Color32 = Color32::from_rgb(0x43, 0x38, 0xCA);
    pub const EMERALD_50: Color32 = Color32::from_rgb(0xEC, 0xFD, 0xF5);
    pub const EMERALD_600: Color32 = Color32::from_rgb(0x05, 0x96, 0x69);
    pub const RED_50: Color32 = Color32::from_rgb(0xFE, 0xF2, 0xF2);
    pub const RED_600: Color32 = Color32::from_rgb(0xDC, 0x26, 0x26);
    pub const AMBER_500: Color32 = Color32::from_rgb(0xF5, 0x9E, 0x0B);
}

pub const SIDEBAR_WIDTH: f32 = 220.0;

/// Light theme in Tailwind colours, set once at start.
pub fn apply(ctx: &egui::Context) {
    use tw::*;
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.window_margin = egui::Margin::same(20);

    use egui::{FontFamily::Proportional, FontId, TextStyle};
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.5, Proportional));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::new(14.5, Proportional));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(24.0, Proportional));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(12.0, Proportional));

    let v = &mut style.visuals;
    v.dark_mode = false;
    v.override_text_color = None;
    v.panel_fill = SLATE_50;
    v.window_fill = WHITE;
    v.window_stroke = Stroke::new(1.0, SLATE_200);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_shadow = egui::Shadow {
        offset: [0, 10],
        blur: 30,
        spread: 0,
        color: Color32::from_black_alpha(40),
    };
    v.popup_shadow = egui::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(25),
    };
    v.menu_corner_radius = CornerRadius::same(10);
    v.faint_bg_color = SLATE_100;
    v.extreme_bg_color = WHITE;
    v.text_edit_bg_color = Some(WHITE);
    v.code_bg_color = SLATE_100;
    v.hyperlink_color = INDIGO_600;
    v.warn_fg_color = AMBER_500;
    v.error_fg_color = RED_600;
    v.selection.bg_fill = INDIGO_500.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, INDIGO_700);
    v.text_cursor.stroke = Stroke::new(2.0, INDIGO_600);

    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = WHITE;
    w.noninteractive.weak_bg_fill = WHITE;
    w.noninteractive.bg_stroke = Stroke::new(1.0, SLATE_200);
    w.noninteractive.fg_stroke = Stroke::new(1.0, SLATE_700);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = SLATE_200;
    w.inactive.weak_bg_fill = WHITE;
    w.inactive.bg_stroke = Stroke::new(1.0, SLATE_300);
    w.inactive.fg_stroke = Stroke::new(1.0, SLATE_800);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = SLATE_300;
    w.hovered.weak_bg_fill = SLATE_100;
    w.hovered.bg_stroke = Stroke::new(1.0, INDIGO_400);
    w.hovered.fg_stroke = Stroke::new(1.5, SLATE_900);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = INDIGO_600;
    w.active.weak_bg_fill = SLATE_200;
    w.active.bg_stroke = Stroke::new(1.0, INDIGO_600);
    w.active.fg_stroke = Stroke::new(2.0, SLATE_900);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = WHITE;
    w.open.weak_bg_fill = SLATE_100;
    w.open.bg_stroke = Stroke::new(1.0, SLATE_300);
    w.open.fg_stroke = Stroke::new(1.0, SLATE_900);
    w.open.corner_radius = r;

    ctx.set_style_of(egui::Theme::Light, style);
}

/// `bg-indigo-600 text-white` button for the main action of a page.
pub fn primary(text: impl Into<String>) -> egui::Button<'static> {
    filled(text, tw::INDIGO_600)
}

/// `bg-emerald-600 text-white` button.
pub fn success(text: impl Into<String>) -> egui::Button<'static> {
    filled(text, tw::EMERALD_600)
}

/// `bg-red-600 text-white` button.
pub fn danger(text: impl Into<String>) -> egui::Button<'static> {
    filled(text, tw::RED_600)
}

fn filled(text: impl Into<String>, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).strong().color(tw::WHITE))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(8)
}

/// White card: `bg-white border border-slate-200 rounded-xl shadow-sm p-4`.
pub fn card<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let r = egui::Frame::new()
        .fill(tw::WHITE)
        .stroke(Stroke::new(1.0, tw::SLATE_200))
        .corner_radius(12)
        .inner_margin(16)
        .shadow(egui::Shadow {
            offset: [0, 1],
            blur: 3,
            spread: 0,
            color: Color32::from_black_alpha(14),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui)
        })
        .inner;
    ui.add_space(10.0);
    r
}

/// Page heading with a grey line of help under it.
pub fn page_title(ui: &mut egui::Ui, title: &str, help: Option<&str>) {
    ui.label(
        RichText::new(title)
            .size(24.0)
            .strong()
            .color(tw::SLATE_900),
    );
    if let Some(help) = help {
        ui.label(RichText::new(help).color(tw::SLATE_500));
    }
    ui.add_space(10.0);
}

/// One menu entry in the sidebar. Returns true when clicked.
pub fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> bool {
    let size = egui::vec2(ui.available_width(), 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let (bg, fg) = if selected {
        (tw::INDIGO_600, tw::WHITE)
    } else if resp.hovered() {
        (tw::SLATE_800, tw::WHITE)
    } else {
        (Color32::TRANSPARENT, tw::SLATE_300)
    };
    let p = ui.painter();
    p.rect_filled(rect, 8.0, bg);
    let font = egui::FontId::proportional(15.0);
    p.text(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        icon,
        egui::FontId::proportional(17.0),
        fg,
    );
    let galley = p.layout(label.to_string(), font, fg, rect.width() - 52.0);
    p.galley(
        egui::pos2(rect.left() + 44.0, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    resp.clicked()
}

/// Small coloured label: `rounded-full px-2 text-xs`.
pub fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(255)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(fg));
        });
}
