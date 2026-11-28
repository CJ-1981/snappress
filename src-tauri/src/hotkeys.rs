//! Global shortcuts from settings.
//!
//! [`apply`] replaces every registered shortcut with the ones configured in
//! the settings: it unregisters the previous set, registers the new one per
//! action, and falls back to the built-in default when the OS refuses one.
//! Shortcut strings are whatever the shortcut plugin's parser accepts —
//! "Ctrl", "Cmd", "CmdOrCtrl", "Alt", "Shift", "Super" as modifiers, the key
//! last, case-insensitive ("Cmd+Shift+6", "Ctrl+Alt+S", "PrintScreen", "F9").

use std::sync::mpsc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::commands::{overlay, record};
use crate::settings::default_hotkeys;
use crate::state::AppState;

/// The actions a shortcut can trigger, in settings-field order.
pub const ACTIONS: [&str; 4] = ["image", "scroll", "record", "stop"];

fn setting_of(settings: &crate::settings::Settings, action: &str) -> String {
    match action {
        "image" => settings.hotkey_image.clone(),
        "scroll" => settings.hotkey_scroll.clone(),
        "record" => settings.hotkey_record.clone(),
        "stop" => settings.hotkey_stop.clone(),
        _ => String::new(),
    }
}

/// What to register, from the settings: one (action, shortcut) per action,
/// empty = disabled.
fn wanted(settings: &crate::settings::Settings) -> Vec<(&'static str, String)> {
    ACTIONS.map(|a| (a, setting_of(settings, a).trim().to_string())).into()
}

fn default_for(action: &str) -> &'static str {
    default_hotkeys()
        .iter()
        .find(|(a, _)| *a == action)
        .map(|(_, v)| *v)
        .unwrap_or("")
}

/// Actions whose non-empty shortcut equals an earlier action's; those cannot
/// all register, so the later ones are skipped with a warning instead of
/// falling back to a default that fires on top of its owner.
fn duplicated_actions(wanted: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for (index, (_, shortcut)) in wanted.iter().enumerate() {
        if shortcut.is_empty() {
            continue;
        }
        for (earlier_action, earlier_shortcut) in wanted.iter().take(index) {
            if earlier_shortcut == shortcut {
                out.push((wanted[index].0, earlier_shortcut.clone()));
                break;
            }
        }
    }
    out
}

fn dispatch(action: &'static str, app: &AppHandle) {
    match action {
        "image" | "scroll" | "record" => overlay::spawn_overlay(app, action),
        "stop" => {
            let _ = record::stop_recording_impl(app);
        }
        _ => {}
    }
}

fn register_one(app: &AppHandle, action: &'static str, shortcut: &str) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                dispatch(action, app);
            }
        })
        .map_err(|e| e.to_string())
}

/// Replace the registered shortcuts with the configured ones. Returns one
/// warning per shortcut that could not be registered (a fallback default is
/// used instead, or nothing when even that fails).
pub fn apply(app: &AppHandle) -> Vec<String> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let wanted = wanted(&settings);
    let duplicates = duplicated_actions(&wanted);

    let app2 = app.clone();
    let (tx, rx) = mpsc::channel();
    let run = move || {
        let gs = app2.global_shortcut();
        let mut warnings = Vec::new();
        let mut registered = Vec::new();

        for (action, old) in app2
            .state::<AppState>()
            .registered_hotkeys
            .lock()
            .unwrap()
            .drain(..)
            .collect::<Vec<_>>()
        {
            if let Err(e) = gs.unregister(old.as_str()) {
                log::warn!("could not unregister the old {action} shortcut '{old}': {e}");
            }
        }

        for (action, shortcut) in &wanted {
            if let Some((_, other)) = duplicates.iter().find(|(a, _)| a == action) {
                warnings.push(format!(
                    "'{shortcut}' is set for both {action} and {other}; {action} keeps its default"
                ));
                continue;
            }
            if shortcut.is_empty() {
                continue; // deliberately disabled
            }
            match register_one(&app2, action, shortcut) {
                Ok(()) => registered.push((*action, shortcut.clone())),
                Err(e) => {
                    let default = default_for(action);
                    warnings.push(format!(
                        "'{shortcut}' could not be registered for {action} ({e}); the default '{default}' is used"
                    ));
                    match register_one(&app2, action, default) {
                        Ok(()) => registered.push((*action, default.to_string())),
                        Err(e2) => warnings.push(format!(
                            "the default '{default}' could not be registered for {action} either: {e2}"
                        )),
                    }
                }
            }
        }

        app2
            .state::<AppState>()
            .registered_hotkeys
            .lock()
            .unwrap()
            .extend(registered.iter().cloned());
        (warnings, registered)
    };

    // Registration touches the plugin's manager, which wants the main thread.
    // `apply` may be called from a worker (settings command) or a startup
    // thread, so hand the work over and wait for the outcome.
    let (warnings, registered) = if std::thread::current().name() == Some("main") {
        run()
    } else {
        app.run_on_main_thread(move || {
            let _ = tx.send(run());
        })
        .map_err(|e| vec![format!("could not update the shortcuts: {e}")])
        .ok()
        .and_then(|()| rx.recv_timeout(Duration::from_secs(5)).ok())
        .unwrap_or_else(|| (vec!["the shortcuts could not be updated".to_string()], Vec::new()))
    };

    for warning in &warnings {
        log::warn!("{warning}");
    }
    let shown: Vec<String> = registered.iter().map(|(a, s)| format!("{a}={s}")).collect();
    log::info!("hotkeys registered: {}", shown.join(", "));
    warnings
}

#[cfg(test)]
mod tests {
    use super::{duplicated_actions, default_for, ACTIONS};
    use crate::settings::default_hotkeys;
    use tauri_plugin_global_shortcut::Shortcut;

    /// Every key token the settings UI can emit.
    fn key_tokens() -> Vec<String> {
        let mut tokens: Vec<String> = Vec::new();
        tokens.extend((b'A'..=b'Z').map(|c| (c as char).to_string()));
        tokens.extend((0..=9).map(|d| d.to_string()));
        tokens.extend((1..=24).map(|n| format!("F{n}")));
        tokens.extend(
            [
                "Space", "PrintScreen", "ScrollLock", "Pause", "Insert", "Delete", "Home", "End",
                "PageUp", "PageDown", "Up", "Down", "Left", "Right", "Minus", "Equal",
                "BracketLeft", "BracketRight", "Backquote", "Backslash", "Comma", "Period",
                "Slash", "Semicolon", "Quote", "Tab",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
        tokens.extend((0..=9).map(|d| format!("Num{d}")));
        tokens
    }

    /// Whatever the UI can produce must be accepted by the shortcut parser,
    /// bare and with modifiers.
    #[test]
    fn every_key_token_parses_as_a_shortcut() {
        let mut failures = Vec::new();
        for token in key_tokens() {
            for combo in [token.clone(), format!("Ctrl+Alt+{token}"), format!("Cmd+Shift+{token}")] {
                if combo.parse::<Shortcut>().is_err() {
                    failures.push(combo);
                }
            }
        }
        assert!(failures.is_empty(), "unparseable shortcuts: {failures:?}");
    }

    #[test]
    fn default_hotkeys_parse_on_this_platform() {
        for (action, shortcut) in default_hotkeys() {
            assert!(
                shortcut.parse::<Shortcut>().is_ok(),
                "the default {action} shortcut '{shortcut}' does not parse"
            );
        }
    }

    #[test]
    fn modifier_order_and_case_do_not_matter() {
        let lower = "cmd+shift+6".parse::<Shortcut>().expect("parse");
        let mixed = "Shift+Cmd+6".parse::<Shortcut>().expect("parse");
        assert_eq!(lower, mixed);
    }

    #[test]
    fn default_for_returns_one_default_per_action() {
        for (action, default) in default_hotkeys() {
            assert_eq!(default_for(action), default);
        }
    }

    #[test]
    fn duplicated_shortcuts_are_flagged_on_the_later_actions() {
        let wanted: Vec<(&'static str, String)> =
            ACTIONS.map(|a| (a, "Ctrl+Alt+P".to_string())).into();
        let flagged: Vec<&str> = duplicated_actions(&wanted).iter().map(|(a, _)| *a).collect();
        assert_eq!(flagged, ["scroll", "record", "stop"]);
        assert!(duplicated_actions(&[("image", String::new())]).is_empty());
    }
}
