//! Native menu bar. Custom items are forwarded to the UI as [`MenuAction`]
//! events; standard items (copy, hide, quit…) are handled by the system.
//!
//! macOS gets the usual app menu. Windows gets a menu bar in the window
//! (File, Edit, View, Help) with the same item ids, so the UI handles both
//! the same way.

use tauri::menu::{AboutMetadataBuilder, Menu, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Runtime};
use tauri_specta::Event;

use crate::events::MenuAction;

pub fn install<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let menu = build(app)?;
    app.set_menu(menu)?;

    app.on_menu_event(|handle, event| {
        let id = event.id().0.clone();
        if let Err(e) = MenuAction(id).emit(handle) {
            log::warn!("could not forward menu action: {e}");
        }
    });
    Ok(())
}

fn item<R: Runtime>(app: &App<R>, id: &str, text: &str, accel: Option<&str>) -> tauri::Result<MenuItem<R>> {
    let mut b = MenuItemBuilder::with_id(id, text);
    if let Some(a) = accel {
        b = b.accelerator(a);
    }
    b.build(app)
}

fn about() -> tauri::menu::AboutMetadata<'static> {
    AboutMetadataBuilder::new()
        .name(Some("Grandie Chess"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .comments(Some("Chess training powered by Leela Chess Zero"))
        .license(Some("GPL-3.0-or-later"))
        .build()
}

#[cfg(target_os = "macos")]
fn build<R: Runtime>(app: &App<R>) -> tauri::Result<Menu<R>> {
    let item = |id: &str, text: &str, accel: Option<&str>| item(app, id, text, accel);

    let app_menu = SubmenuBuilder::new(app, "Grandie Chess")
        .about(Some(about()))
        .item(&item("check_updates", "Check for Updates…", None)?)
        .separator()
        .item(&item("settings", "Settings…", Some("CmdOrCtrl+,"))?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&item("new_game", "New Game", Some("CmdOrCtrl+N"))?)
        .item(&item("new_analysis", "New Analysis Board", Some("CmdOrCtrl+Shift+N"))?)
        .separator()
        .item(&item("open_pgn", "Open PGN…", Some("CmdOrCtrl+O"))?)
        .item(&item("save_pgn", "Save PGN…", Some("CmdOrCtrl+S"))?)
        .separator()
        .item(&item("close_tab", "Close Tab", Some("CmdOrCtrl+W"))?)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&item("copy_fen", "Copy FEN", Some("CmdOrCtrl+Shift+C"))?)
        .item(&item("paste_fen", "Paste FEN or PGN", Some("CmdOrCtrl+Shift+V"))?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .item(&item("flip_board", "Flip Board", None)?)
        .item(&item("toggle_engine", "Start/Stop Engine", None)?)
        .separator()
        .fullscreen()
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Window").minimize().maximize().separator().close_window().bring_all_to_front().build()?;

    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(&item("help_shortcuts", "Keyboard Shortcuts", Some("CmdOrCtrl+/"))?)
        .item(&item("help_credits", "Credits and Licences", None)?)
        .build()?;

    MenuBuilder::new(app).items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu]).build()
}

/// Windows layout. The webview handles cut, copy, paste, undo and select all
/// by itself, so the Edit menu holds only Grandie's own items.
#[cfg(not(target_os = "macos"))]
fn build<R: Runtime>(app: &App<R>) -> tauri::Result<Menu<R>> {
    let item = |id: &str, text: &str, accel: Option<&str>| item(app, id, text, accel);

    let file_menu = SubmenuBuilder::new(app, "&File")
        .item(&item("new_game", "&New Game", Some("CmdOrCtrl+N"))?)
        .item(&item("new_analysis", "New &Analysis Board", Some("CmdOrCtrl+Shift+N"))?)
        .separator()
        .item(&item("open_pgn", "&Open PGN…", Some("CmdOrCtrl+O"))?)
        .item(&item("save_pgn", "&Save PGN…", Some("CmdOrCtrl+S"))?)
        .separator()
        .item(&item("settings", "Se&ttings…", Some("CmdOrCtrl+,"))?)
        .separator()
        .item(&item("close_tab", "&Close Tab", Some("CmdOrCtrl+W"))?)
        .separator()
        .quit_with_text("E&xit")
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "&Edit")
        .item(&item("copy_fen", "&Copy FEN", Some("CmdOrCtrl+Shift+C"))?)
        .item(&item("paste_fen", "&Paste FEN or PGN", Some("CmdOrCtrl+Shift+V"))?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "&View")
        .item(&item("flip_board", "&Flip Board", None)?)
        .item(&item("toggle_engine", "Start/Stop &Engine", None)?)
        .build()?;

    let help_menu = SubmenuBuilder::new(app, "&Help")
        .item(&item("help_shortcuts", "&Keyboard Shortcuts", Some("CmdOrCtrl+/"))?)
        .item(&item("help_credits", "C&redits and Licences", None)?)
        .separator()
        .item(&item("check_updates", "Check for &Updates…", None)?)
        .about_with_text("&About Grandie Chess", Some(about()))
        .build()?;

    MenuBuilder::new(app).items(&[&file_menu, &edit_menu, &view_menu, &help_menu]).build()
}
