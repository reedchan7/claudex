//! claudex-bar: a desktop-widget style floating window that renders
//! `claudex usage --all --json` snapshots on a poll timer.
//!
//! The window is transparent, borderless, and pinned below normal windows
//! (macOS desktop level); a tray icon offers manual refresh, click-through
//! toggle, and quit. All data comes from the claudex CLI — this process never
//! touches the network or credentials itself.

mod config;
mod format;
mod poller;
mod tray;

use std::time::{Duration, Instant};

use clap::Parser;
use eframe::egui::{
    self, CentralPanel, Color32, CornerRadius, Frame, Margin, ProgressBar, RichText, Sense, Stroke,
    Ui, Vec2,
};
use egui::WindowLevel;
use egui::viewport::{ViewportBuilder, ViewportCommand};

use crate::commands::status::Provider;
use crate::snapshot::{ProviderSnapshot, ProviderState, Row, Snapshot};

use format::{age_label, refreshed_detail};
use poller::{PollResult, Poller};

const WINDOW_WIDTH: f32 = 300.0;
const WINDOW_PAD: f32 = 12.0;
const MIN_INTERVAL_SECS: u64 = 60;

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
    click_through: bool,
    interval: Duration,
    config: config::BarConfig,
    last_pos: Option<(f32, f32)>,
    pos_changed_at: Option<Instant>,
    desired_height: f32,
    first_frame: bool,
}

impl BarApp {
    fn new(
        poller: Poller,
        tray: Option<tray::Tray>,
        config: config::BarConfig,
        click_through: bool,
        interval_secs: u64,
    ) -> Self {
        Self {
            poller,
            tray,
            latest: None,
            last_error: None,
            last_success: None,
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
}

impl eframe::App for BarApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first_frame {
            #[cfg(target_os = "macos")]
            macos::hide_dock_icon();
            self.first_frame = false;
        }

        for result in self.poller.drain() {
            match result {
                PollResult::Ok(snapshot) => {
                    self.latest = Some(snapshot);
                    self.last_error = None;
                    self.last_success = Some(Instant::now());
                }
                PollResult::Err(e) => self.last_error = Some(e),
            }
        }

        if let Some(tray) = &self.tray {
            for command in tray.drain_commands() {
                match command {
                    tray::TrayCommand::Refresh => self.poller.refresh_now(),
                    tray::TrayCommand::ToggleClickThrough => {
                        self.click_through = !self.click_through;
                        ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(
                            self.click_through,
                        ));
                        tray.set_click_through(self.click_through);
                    }
                    tray::TrayCommand::Quit => {
                        self.save_position();
                        std::process::exit(0);
                    }
                }
            }
        }

        self.track_position(ctx);
        // Keep countdowns and the "updated … ago" footer fresh between polls.
        ctx.request_repaint_after(Duration::from_secs(30));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut content_height = 0.0;
        CentralPanel::default().frame(panel_frame()).show(ui, |ui| {
            self.content_ui(ui);
            // Drag anywhere on the card to move the window.
            let drag = ui.interact(ui.max_rect(), egui::Id::new("window-drag"), Sense::drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            content_height = ui.min_rect().height();
        });

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
    fn content_ui(&mut self, ui: &mut Ui) {
        match &self.latest {
            None => {
                ui.label(RichText::new("Loading usage…").size(11.0).weak());
            }
            Some(snapshot) => {
                for (index, provider) in snapshot.providers.iter().enumerate() {
                    if index > 0 {
                        ui.add_space(10.0);
                    }
                    provider_ui(ui, provider);
                }
            }
        }

        ui.add_space(8.0);
        ui.separator();
        let updated = self
            .last_success
            .map(|at| age_label(at.elapsed().as_secs()))
            .unwrap_or_else(|| "never".to_string());
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "Updated {updated} · every {}m",
                    self.interval.as_secs() / 60
                ))
                .size(9.5)
                .weak(),
            );
        });
        if let Some(error) = &self.last_error {
            ui.label(
                RichText::new(format!("refresh failed: {error}"))
                    .size(9.5)
                    .color(Color32::from_rgb(235, 87, 87)),
            );
        }
    }
}

fn panel_frame() -> Frame {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(20, 20, 24, 216))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(14)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(WINDOW_PAD as i8))
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

fn provider_ui(ui: &mut Ui, provider: &ProviderSnapshot) {
    let (r, g, b) = provider.accent;
    ui.label(
        RichText::new(&provider.label)
            .size(13.0)
            .color(Color32::from_rgb(r, g, b))
            .strong(),
    );

    match &provider.state {
        ProviderState::Ok { blocks } => {
            for block in blocks {
                block_ui(ui, block);
            }
        }
        ProviderState::Unavailable {
            heading, next_step, ..
        } => {
            ui.add_space(4.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
            ui.painter().rect_filled(rect, 3, Color32::from_gray(70));
            ui.add_space(2.0);
            ui.label(
                RichText::new(heading)
                    .size(10.5)
                    .color(Color32::from_rgb(245, 198, 106)),
            );
            ui.label(RichText::new(next_step).size(9.5).weak());
        }
    }
}

fn block_ui(ui: &mut Ui, block: &crate::snapshot::Block) {
    if let Some(title) = &block.title {
        ui.add_space(6.0);
        ui.label(
            RichText::new(title)
                .size(11.0)
                .color(Color32::from_gray(205)),
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
                    let bar_width = (ui.available_width() - 52.0).max(60.0);
                    ui.add_sized(
                        [bar_width, 6.0],
                        ProgressBar::new((*percent / 100.0) as f32)
                            .fill(utilization_color(*percent))
                            .desired_height(6.0)
                            .corner_radius(3),
                    );
                    ui.label(
                        RichText::new(text)
                            .size(10.0)
                            .color(Color32::from_gray(190)),
                    );
                });
                if let Some(detail) = detail {
                    ui.label(
                        RichText::new(refreshed_detail(detail, resets_at.as_deref()))
                            .size(9.5)
                            .weak(),
                    );
                }
            }
            Row::Text { text } => {
                ui.label(
                    RichText::new(text)
                        .size(10.0)
                        .color(Color32::from_gray(180)),
                );
            }
        }
    }
}
