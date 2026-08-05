//! Menu-bar (tray) icon for claudex-bar: refresh, click-through toggle, quit.

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

pub enum TrayCommand {
    Refresh,
    ToggleClickThrough,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    refresh_id: MenuId,
    click_through_item: CheckMenuItem,
    click_through_id: MenuId,
    quit_id: MenuId,
}

impl Tray {
    /// Build the tray icon and menu. Returns None (with a stderr note) when
    /// the platform tray is unavailable — the window still works.
    pub fn new(click_through: bool) -> Option<Self> {
        match Self::build(click_through) {
            Ok(tray) => Some(tray),
            Err(e) => {
                eprintln!("note: tray icon unavailable: {e}");
                None
            }
        }
    }

    fn build(click_through: bool) -> Result<Self, String> {
        let refresh_item = MenuItem::new("Refresh Now", true, None);
        let click_through_item = CheckMenuItem::new("Click-through", true, click_through, None);
        let quit_item = MenuItem::new("Quit claudex-bar", true, None);

        let menu = Menu::new();
        menu.append(&refresh_item).map_err(|e| e.to_string())?;
        menu.append(&click_through_item)
            .map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;

        let icon = TrayIconBuilder::new()
            .with_tooltip("claudex bar")
            .with_menu(Box::new(menu))
            .with_icon(make_icon())
            .with_icon_as_template(true)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            _icon: icon,
            refresh_id: refresh_item.id().clone(),
            click_through_id: click_through_item.id().clone(),
            click_through_item,
            quit_id: quit_item.id().clone(),
        })
    }

    pub fn drain_commands(&self) -> Vec<TrayCommand> {
        MenuEvent::receiver()
            .try_iter()
            .filter_map(|event| {
                if event.id == self.refresh_id {
                    Some(TrayCommand::Refresh)
                } else if event.id == self.click_through_id {
                    Some(TrayCommand::ToggleClickThrough)
                } else if event.id == self.quit_id {
                    Some(TrayCommand::Quit)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn set_click_through(&self, enabled: bool) {
        self.click_through_item.set_checked(enabled);
    }
}

/// Three ascending bar-chart columns on a transparent 22×22 template icon.
fn icon_rgba() -> Vec<u8> {
    const SIZE: usize = 22;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    // (left x, column height)
    for (x0, height) in [(4usize, 9usize), (10usize, 15usize), (16usize, 6usize)] {
        for y in (SIZE - 2 - height)..(SIZE - 2) {
            for x in x0..x0 + 3 {
                let i = (y * SIZE + x) * 4;
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            }
        }
    }
    rgba
}

fn make_icon() -> tray_icon::Icon {
    const SIZE: u32 = 22;
    tray_icon::Icon::from_rgba(icon_rgba(), SIZE, SIZE).expect("hard-coded icon is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_opaque_columns_and_transparent_background() {
        let rgba = icon_rgba();
        // Top-left corner stays transparent.
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        // At least one fully white pixel inside a column.
        assert!(rgba.chunks_exact(4).any(|px| px == [255, 255, 255, 255]));
    }
}
