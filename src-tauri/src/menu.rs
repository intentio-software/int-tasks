//! The native application menu.
//!
//! Menu items do not act on their own: each one emits a `menu-action` event
//! carrying its id, and the Angular side runs the same handler the in-app
//! controls use. That keeps one implementation of "new task" or "start focus
//! session" rather than a native copy and a web copy that drift apart.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime};

/// Event the frontend listens on. The payload is the menu item id.
pub const MENU_EVENT: &str = "menu-action";

/// Build the whole menu bar.
pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    // The About item opens the app's own dialog rather than the system panel,
    // so no `AboutMetadata` is needed here.

    // --- application menu (macOS only; ignored elsewhere) -------------------
    let app_menu = Submenu::with_items(
        app,
        "Intentio Tasks",
        true,
        &[
            &MenuItem::with_id(app, "about", "About Intentio Tasks", true, None::<&str>)?,
            &MenuItem::with_id(app, "check-updates", "Check for Updates…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    // --- File ---------------------------------------------------------------
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "new-task", "New Task", true, Some("CmdOrCtrl+N"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "new-board", "New Board…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    // --- Edit ---------------------------------------------------------------
    //
    // Predefined items, unlike Mind Map's: every editable surface here is an
    // ordinary text input, so the webview's own handling is the correct one.
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    // --- View ---------------------------------------------------------------
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "view-today", "Today", true, Some("CmdOrCtrl+1"))?,
            &MenuItem::with_id(app, "view-board", "Board", true, Some("CmdOrCtrl+2"))?,
            &MenuItem::with_id(app, "view-flow", "Flow", true, Some("CmdOrCtrl+3"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "manage-labels", "Projects & Tags…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "toggle-theme", "Switch Theme", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    // --- Timer --------------------------------------------------------------
    let timer_menu = Submenu::with_items(
        app,
        "Timer",
        true,
        &[
            &MenuItem::with_id(app, "timer-start", "Start Focus Session", true, Some("CmdOrCtrl+T"))?,
            &MenuItem::with_id(app, "timer-break", "Start Break", true, None::<&str>)?,
            &MenuItem::with_id(app, "timer-stop", "Stop Timer", true, Some("CmdOrCtrl+Shift+T"))?,
        ],
    )?;

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[&PredefinedMenuItem::minimize(app, None)?, &PredefinedMenuItem::maximize(app, None)?],
    )?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &MenuItem::with_id(app, "about", "About Intentio Tasks", true, None::<&str>)?,
            &MenuItem::with_id(app, "check-updates", "Check for Updates…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "website", "Intentio Software", true, None::<&str>)?,
        ],
    )?;

    Menu::with_items(
        app,
        &[&app_menu, &file_menu, &edit_menu, &view_menu, &timer_menu, &window_menu, &help_menu],
    )
}

/// Install the menu and forward every click to the frontend.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build(app)?;
    app.set_menu(menu)?;

    app.on_menu_event(|app, event| {
        let id = event.id().as_ref().to_string();
        // The window may already be closing; a failed emit is not worth logging.
        let _ = app.emit(MENU_EVENT, id);
    });

    Ok(())
}
