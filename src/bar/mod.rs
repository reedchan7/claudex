//! claudex-bar: a desktop-widget style floating window that renders
//! `claudex usage --all --json` snapshots on a poll timer.
//!
//! The window is transparent, borderless, and pinned below normal windows
//! (macOS desktop level); a tray icon offers manual refresh, show/hide,
//! click-through toggle, and quit. All data comes from the claudex CLI — this
//! process never touches the network or credentials itself.

mod config;
mod format;
mod poller;
mod tray;

use std::time::{Duration, Instant};

use clap::Parser;
use eframe::egui::{
    self, Align, CentralPanel, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, RichText,
    Sense, Spinner, Stroke, Ui, Vec2,
};
use egui::WindowLevel;
use egui::viewport::{ViewportBuilder, ViewportCommand};

use crate::commands::status::Provider;
use crate::snapshot::{ProviderSnapshot, ProviderState, Row, Snapshot};

use config::BarConfig;
use format::{age_label, refreshed_detail};
use poller::{PollEvent, Poller};

const WINDOW_WIDTH: f32 = 340.0;
const WINDOW_PAD: f32 = 14.0;
const MIN_INTERVAL_SECS: u64 = 60;

const SIZE_TITLE: f32 = 13.5;
const SIZE_PROVIDER: f32 = 14.0;
const SIZE_BLOCK: f32 = 12.0;
const SIZE_BAR_TEXT: f32 = 11.0;
const SIZE_DETAIL: f32 = 10.5;
const SIZE_FOOTER: f32 = 10.0;
const BAR_HEIGHT: f32 = 7.0;

/// Desktop widget showing claudex usage
#[derive(Parser)]
#[command(name = "claudex-bar", version)]
struct BarArgs {
    /// Skip one or more providers (repeatable or comma-separated)
    #[arg(long = "skip", value_name = "AGENT", action = clap::ArgAction::Append, value_delimiter = ',')]
    skip: Vec<String>,
    /// Poll interval in seconds (minimum 60)
    #[arg(long, default_value_t = 300, value_name = "SECS")]
    interval: u64,
    /// Start with click-through enabled (window ignores the mouse; toggle via tray menu)
    #[arg(long)]
    click_through: bool,
}

/// Text/card colors, selected from the OS appearance (light or dark).
#[derive(Clone, Copy)]
struct Palette {
    card_fill: Color32,
    card_stroke: Color32,
    primary: Color32,
    secondary: Color32,
    faint: Color32,
    empty_bar: Color32,
    warn: Color32,
    error: Color32,
}

impl Palette {
    fn of(ctx: &egui::Context) -> Self {
        match ctx.system_theme() {
            Some(egui::Theme::Light) => Self::light(),
            _ => Self::dark(),
        }
    }

    fn dark() -> Self {
        Self {
            card_fill: Color32::from_rgba_unmultiplied(22, 22, 27, 236),
            card_stroke: Color32::from_white_alpha(18),
            primary: Color32::from_gray(235),
            secondary: Color32::from_gray(205),
            faint: Color32::from_gray(150),
            empty_bar: Color32::from_gray(70),
            warn: Color32::from_rgb(245, 198, 106),
            error: Color32::from_rgb(235, 87, 87),
        }
    }

    fn light() -> Self {
        Self {
            card_fill: Color32::from_rgba_unmultiplied(250, 250, 252, 238),
            card_stroke: Color32::from_black_alpha(24),
            primary: Color32::from_gray(25),
            secondary: Color32::from_gray(55),
            faint: Color32::from_gray(105),
            empty_bar: Color32::from_gray(215),
            warn: Color32::from_rgb(168, 118, 20),
            error: Color32::from_rgb(200, 45, 45),
        }
    }
}

pub fn run() -> eframe::Result<()> {
    let args = BarArgs::parse();

    for name in &args.skip {
        if Provider::from_skip_name(name).is_none() {
            eprintln!("unknown provider '{name}'");
            std::process::exit(2);
        }
    }

    let interval = args.interval.max(MIN_INTERVAL_SECS);
    if args.interval < MIN_INTERVAL_SECS {
        eprintln!("note: interval clamped to {MIN_INTERVAL_SECS}s minimum");
    }

    let claudex_bin = match poller::resolve_claudex_bin() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("✗ {e}");
            std::process::exit(1);
        }
    };

    let saved = config::load();
    let mut viewport = ViewportBuilder::default()
        .with_title("claudex bar")
        .with_inner_size([WINDOW_WIDTH, 240.0])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_has_shadow(false)
        .with_window_level(WindowLevel::AlwaysOnBottom)
        .with_mouse_passthrough(args.click_through);
    if let Some((x, y)) = saved.position() {
        viewport = viewport.with_position([x, y]);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let click_through = args.click_through;
    let skip = args.skip.clone();
    eframe::run_native(
        "claudex-bar",
        options,
        Box::new(move |cc| {
            #[cfg(target_os = "macos")]
            macos::hide_dock_icon();
            install_system_font(&cc.egui_ctx);

            let tray = tray::Tray::new(click_through);
            let poller = Poller::start(
                claudex_bin,
                skip,
                Duration::from_secs(interval),
                cc.egui_ctx.clone(),
            );
            Ok(Box::new(BarApp::new(
                poller,
                tray,
                saved,
                click_through,
                interval,
            )))
        }),
    )
}

/// Prefer the macOS system font (San Francisco) over egui's bundled fonts so
/// the card matches the platform's text rendering. Silently keeps egui's
/// defaults when the font files can't be read.
fn install_system_font(ctx: &egui::Context) {
    for path in [
        "/System/Library/Fonts/SFNS.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
    ] {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "system-ui".to_string(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "system-ui".to_string());
        ctx.set_fonts(fonts);
        return;
    }
}

#[cfg(target_os = "macos")]
mod macos {
    /// Run as an accessory app: no Dock icon, no menu bar, no Cmd-Tab entry.
    pub fn hide_dock_icon() {
        use objc2::MainThreadMarker;
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        if let Some(mtm) = MainThreadMarker::new() {
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
    }
}

struct BarApp {
    poller: Poller,
    tray: Option<tray::Tray>,
    latest: Option<Snapshot>,
    last_error: Option<String>,
    last_success: Option<Instant>,
    loading: bool,
    hidden: bool,
    click_through: bool,
    interval: Duration,
    config: BarConfig,
    last_pos: Option<(f32, f32)>,
    pos_changed_at: Option<Instant>,
    desired_height: f32,
    first_frame: bool,
}

impl BarApp {
    fn new(
        poller: Poller,
        tray: Option<tray::Tray>,
        config: BarConfig,
        click_through: bool,
        interval_secs: u64,
    ) -> Self {
        Self {
            poller,
            tray,
            latest: None,
            last_error: None,
            last_success: None,
            loading: true, // first poll starts immediately
            hidden: false,
            click_through,
            interval: Duration::from_secs(interval_secs),
            config,
            last_pos: None,
            pos_changed_at: None,
            desired_height: 0.0,
            first_frame: true,
        }
    }

    fn save_position(&mut self) {
        if let Some((x, y)) = self.last_pos {
            self.config.x = Some(x);
            self.config.y = Some(y);
            config::save(&self.config);
        }
    }

    fn track_position(&mut self, ctx: &egui::Context) {
        let pos = ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)));
        if pos != self.last_pos {
            if pos.is_some() {
                self.pos_changed_at = Some(Instant::now());
            }
            self.last_pos = pos;
        }
        if let Some(since) = self.pos_changed_at
            && since.elapsed() > Duration::from_secs(2)
        {
            self.save_position();
            self.pos_changed_at = None;
        }
    }

    fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.hidden = !visible;
        ctx.send_viewport_cmd(ViewportCommand::Visible(visible));
    }
}

impl eframe::App for BarApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first_frame {
            #[cfg(target_os = "macos")]
            macos::hide_dock_icon();
            self.first_frame = false;
        }

        for event in self.poller.drain() {
            match event {
                PollEvent::Started => self.loading = true,
                PollEvent::Ok(snapshot) => {
                    self.latest = Some(snapshot);
                    self.last_error = None;
                    self.last_success = Some(Instant::now());
                    self.loading = false;
                }
                PollEvent::Err(e) => {
                    self.last_error = Some(e);
                    self.loading = false;
                }
            }
        }

        let tray_commands = self
            .tray
            .as_ref()
            .map(|tray| tray.drain_commands())
            .unwrap_or_default();
        for command in tray_commands {
            match command {
                tray::TrayCommand::Refresh => self.poller.refresh_now(),
                tray::TrayCommand::ToggleClickThrough => {
                    self.click_through = !self.click_through;
                    ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(self.click_through));
                    if let Some(tray) = &self.tray {
                        tray.set_click_through(self.click_through);
                    }
                }
                tray::TrayCommand::ToggleVisible => {
                    let visible = self.hidden;
                    self.set_visible(ctx, visible);
                }
                tray::TrayCommand::Quit => {
                    self.save_position();
                    std::process::exit(0);
                }
            }
        }

        self.track_position(ctx);
        // Keep countdowns and the "updated … ago" footer fresh between polls.
        ctx.request_repaint_after(Duration::from_secs(30));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let palette = Palette::of(ui.ctx());
        let content_height = CentralPanel::default()
            .frame(panel_frame(palette))
            .show(ui, |ui| {
                let content = ui.vertical(|ui| {
                    self.content_ui(ui, palette);
                });
                // Drag anywhere on the card to move the window.
                let drag = ui.interact(ui.max_rect(), egui::Id::new("window-drag"), Sense::drag());
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
                content.response.rect.height()
            })
            .inner;

        let target_height = content_height + 2.0 * WINDOW_PAD;
        if (target_height - self.desired_height).abs() > 4.0 {
            self.desired_height = target_height;
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                    WINDOW_WIDTH,
                    target_height,
                )));
        }
    }
}

impl BarApp {
    fn content_ui(&mut self, ui: &mut Ui, palette: Palette) {
        self.header_ui(ui, palette);
        ui.add_space(6.0);

        match &self.latest {
            None => {
                ui.label(
                    RichText::new("Loading usage…")
                        .size(SIZE_DETAIL)
                        .color(palette.faint),
                );
            }
            Some(snapshot) => {
                for (index, provider) in snapshot.providers.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(if self.config.mini { 4.0 } else { 12.0 });
                    }
                    if self.config.mini {
                        mini_provider_ui(ui, palette, provider);
                    } else {
                        provider_ui(ui, palette, provider, &mut self.config);
                    }
                }
            }
        }

        ui.add_space(10.0);
        ui.separator();
        let updated = self
            .last_success
            .map(|at| age_label(at.elapsed().as_secs()))
            .unwrap_or_else(|| "never".to_string());
        ui.label(
            RichText::new(format!(
                "Updated {updated} · every {}m",
                self.interval.as_secs() / 60
            ))
            .size(SIZE_FOOTER)
            .color(palette.faint),
        );
        if let Some(error) = &self.last_error {
            ui.label(
                RichText::new(format!("refresh failed: {error}"))
                    .size(SIZE_FOOTER)
                    .color(palette.error),
            );
        }
    }

    fn header_ui(&mut self, ui: &mut Ui, palette: Palette) {
        ui.horizontal(|ui| {
            let mini_toggle = if self.config.mini { "▸" } else { "▾" };
            if icon_button(ui, mini_toggle, palette).clicked() {
                self.config.mini = !self.config.mini;
                config::save(&self.config);
            }
            ui.label(
                RichText::new("Agent Usage")
                    .size(SIZE_TITLE)
                    .color(palette.primary)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, "×", palette).clicked() {
                    self.set_visible(ui.ctx(), false);
                }
                if self.loading {
                    ui.add(Spinner::new().size(13.0).color(palette.secondary));
                } else if icon_button(ui, "↻", palette).clicked() {
                    self.poller.refresh_now();
                    self.loading = true;
                }
            });
        });
    }
}

fn panel_frame(palette: Palette) -> Frame {
    Frame::new()
        .fill(palette.card_fill)
        .stroke(Stroke::new(1.0, palette.card_stroke))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(WINDOW_PAD as i8))
}

fn icon_button(ui: &mut Ui, text: &str, palette: Palette) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(13.0).color(palette.secondary)).frame(false))
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn utilization_color(percent: f64) -> Color32 {
    if percent < 50.0 {
        Color32::from_rgb(142, 192, 124)
    } else if percent < 80.0 {
        Color32::from_rgb(229, 192, 123)
    } else {
        Color32::from_rgb(235, 87, 87)
    }
}

/// Highest utilization across a provider's bar rows, for collapsed summaries.
fn max_percent(provider: &ProviderSnapshot) -> Option<f64> {
    match &provider.state {
        ProviderState::Ok { blocks } => blocks
            .iter()
            .flat_map(|block| &block.rows)
            .filter_map(|row| match row {
                Row::Bar { percent, .. } => Some(*percent),
                Row::Text { .. } => None,
            })
            .reduce(f64::max),
        ProviderState::Unavailable { .. } => None,
    }
}

fn provider_ui(ui: &mut Ui, palette: Palette, provider: &ProviderSnapshot, config: &mut BarConfig) {
    let (r, g, b) = provider.accent;
    let accent = Color32::from_rgb(r, g, b);
    let collapsed = config.collapsed.iter().any(|id| id == &provider.id);
    let arrow = if collapsed { "▸" } else { "▾" };

    let header = ui
        .add(
            egui::Label::new(
                RichText::new(format!("{arrow} {}", provider.label))
                    .size(SIZE_PROVIDER)
                    .color(accent)
                    .strong(),
            )
            .sense(Sense::click()),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    if header.clicked() {
        if collapsed {
            config.collapsed.retain(|id| id != &provider.id);
        } else {
            config.collapsed.push(provider.id.clone());
        }
        config::save(config);
    }

    if collapsed {
        let summary = match max_percent(provider) {
            Some(max) => format!("peak {:.0}% used", max),
            None => "unavailable".to_string(),
        };
        ui.label(
            RichText::new(summary)
                .size(SIZE_DETAIL)
                .color(palette.faint),
        );
        return;
    }

    match &provider.state {
        ProviderState::Ok { blocks } => {
            for block in blocks {
                block_ui(ui, palette, block);
            }
        }
        ProviderState::Unavailable {
            heading, next_step, ..
        } => {
            ui.add_space(4.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), BAR_HEIGHT), Sense::hover());
            ui.painter().rect_filled(rect, 3, palette.empty_bar);
            ui.add_space(3.0);
            ui.label(RichText::new(heading).size(SIZE_DETAIL).color(palette.warn));
            ui.label(
                RichText::new(next_step)
                    .size(SIZE_FOOTER)
                    .color(palette.faint),
            );
        }
    }
}

/// One-line-per-provider compact rendering for mini mode.
fn mini_provider_ui(ui: &mut Ui, palette: Palette, provider: &ProviderSnapshot) {
    let (r, g, b) = provider.accent;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 3.5, Color32::from_rgb(r, g, b));
        ui.label(
            RichText::new(&provider.label)
                .size(SIZE_BAR_TEXT)
                .color(palette.secondary),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            match &provider.state {
                ProviderState::Ok { .. } => {
                    let max = max_percent(provider).unwrap_or(0.0);
                    ui.label(
                        RichText::new(format!("{max:.0}%"))
                            .size(SIZE_BAR_TEXT)
                            .color(utilization_color(max)),
                    );
                }
                ProviderState::Unavailable { .. } => {
                    ui.label(RichText::new("!").size(SIZE_BAR_TEXT).color(palette.warn));
                }
            }
        });
    });
}

fn block_ui(ui: &mut Ui, palette: Palette, block: &crate::snapshot::Block) {
    if let Some(title) = &block.title {
        ui.add_space(7.0);
        ui.label(
            RichText::new(title)
                .size(SIZE_BLOCK)
                .color(palette.secondary),
        );
    }
    for row in &block.rows {
        match row {
            Row::Bar {
                percent,
                text,
                detail,
                resets_at,
            } => {
                ui.horizontal(|ui| {
                    let bar_width = (ui.available_width() - 62.0).max(60.0);
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(bar_width, BAR_HEIGHT), Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(rect, 3, palette.empty_bar);
                    let fill_width = bar_width * (*percent as f32 / 100.0).clamp(0.0, 1.0);
                    if fill_width > 1.0 {
                        let fill =
                            egui::Rect::from_min_size(rect.min, Vec2::new(fill_width, BAR_HEIGHT));
                        let radius = ((fill_width / 2.0).min(3.0)) as u8;
                        painter.rect_filled(fill, radius, utilization_color(*percent));
                    }
                    ui.label(
                        RichText::new(text)
                            .size(SIZE_BAR_TEXT)
                            .color(palette.secondary),
                    );
                });
                if let Some(detail) = detail {
                    ui.label(
                        RichText::new(refreshed_detail(detail, resets_at.as_deref()))
                            .size(SIZE_DETAIL)
                            .color(palette.faint),
                    );
                }
            }
            Row::Text { text } => {
                ui.label(RichText::new(text).size(SIZE_BAR_TEXT).color(palette.faint));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::Block;

    fn provider_with_bars(percents: &[f64]) -> ProviderSnapshot {
        let blocks = percents
            .iter()
            .map(|percent| {
                Block::titled(
                    "limit",
                    vec![Row::bar(
                        *percent,
                        format!("{percent:.0}% used"),
                        None,
                        None,
                    )],
                )
            })
            .collect();
        ProviderSnapshot::ok(Provider::Claude, blocks)
    }

    #[test]
    fn max_percent_picks_the_highest_bar() {
        let provider = provider_with_bars(&[12.0, 86.5, 34.0]);
        assert_eq!(max_percent(&provider), Some(86.5));
    }

    #[test]
    fn max_percent_is_none_without_bars_or_when_unavailable() {
        let empty = ProviderSnapshot::ok(Provider::Claude, vec![Block::untitled(vec![])]);
        assert_eq!(max_percent(&empty), None);

        let status = crate::commands::status::ProviderStatus {
            heading: "h".to_string(),
            detail: "d".to_string(),
            next_step: "n".to_string(),
            details: None,
        };
        let unavailable = ProviderSnapshot::unavailable(Provider::Claude, &status);
        assert_eq!(max_percent(&unavailable), None);
    }
}
