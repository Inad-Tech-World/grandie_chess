//! Native macOS menu bar. Custom items are forwarded to the UI as
//! [`MenuAction`] events; standard items (copy, hide, quit…) are handled by macOS.

use tauri::menu::{AboutMetadataBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Runtime};
use tauri_specta::Event;

use crate::events::MenuAction;

pub fn install<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let item = |id: &str, text: &str, accel: Option<&str>| {
        let mut b = MenuItemBuilder::with_id(id, text);
        if let Some(a) = accel {
            b = b.accelerator(a);
        }
        b.build(app)
    };

    let about = AboutMetadataBuilder::new()
        .name(Some("Grandie Chess"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .comments(Some("Chess training powered by Leela Chess Zero"))
        .license(Some("GPL-3.0-or-later"))
        .build();

    let app_menu = SubmenuBuilder::new(app, "Grandie Chess")
        .about(Some(about))
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

    let menu = MenuBuilder::new(app).items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu]).build()?;
    app.set_menu(menu)?;

    app.on_menu_event(|handle, event| {
        let id = event.id().0.clone();
        if let Err(e) = MenuAction(id).emit(handle) {
            log::warn!("could not forward menu action: {e}");
        }
    });
    Ok(())
}
