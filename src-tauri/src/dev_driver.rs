//! A test driver for the real app, in debug builds only: start the app with
//! `GRANDIE_DEV_DRIVER=<folder>` and every `<folder>/in/<name>.js` file is run
//! in the main window (as the body of an async function; it may `return` a
//! value), then deleted. The outcome is written to `<folder>/out/<name>.json`
//! as `{"ok": true, "result": …}` or `{"ok": false, "error": "…"}`.
//! `scripts/drive.py` sends scripts and waits for their answers.
//!
//! It exists so the app can be exercised end to end against real engines,
//! files and network on a developer's machine, which the Playwright tests
//! (mock backend) can't do. Release builds don't include it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Listener, Manager};

const EVENT: &str = "dev-driver";

pub fn start(app: &AppHandle, dir: PathBuf) {
    let inbox = dir.join("in");
    let outbox = dir.join("out");
    if let Err(e) = std::fs::create_dir_all(&inbox).and_then(|_| std::fs::create_dir_all(&outbox)) {
        log::error!("dev driver: can't create {}: {e}", dir.display());
        return;
    }
    log::warn!("dev driver: running scripts from {}", inbox.display());

    // Results come back from the page as an event.
    let out = outbox.clone();
    app.listen_any(EVENT, move |event| {
        let payload = event.payload();
        let id = serde_json::from_str::<serde_json::Value>(payload)
            .ok()
            .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(str::to_string))
            .unwrap_or_else(|| "unknown".into());
        let tmp = out.join(format!("{id}.json.part"));
        if std::fs::write(&tmp, payload).is_ok() {
            let _ = std::fs::rename(&tmp, out.join(format!("{id}.json")));
        }
    });

    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(150));
        for (id, path) in scripts(&inbox) {
            let Ok(body) = std::fs::read_to_string(&path) else { continue };
            let _ = std::fs::remove_file(&path);
            let Some(window) = app.get_webview_window("main") else { continue };
            if let Err(e) = window.eval(wrap(&id, &body)) {
                log::error!("dev driver: eval failed: {e}");
            }
        }
    });
}

/// Waiting scripts, oldest name first.
fn scripts(inbox: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(inbox) else { return vec![] };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "js"))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p)))
        .collect();
    found.sort();
    found
}

fn wrap(id: &str, body: &str) -> String {
    let id = serde_json::to_string(id).unwrap_or_else(|_| "\"unknown\"".into());
    format!(
        r#"(async () => {{
  const send = (payload) => window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {{ event: "{EVENT}", payload: {{ id: {id}, ...payload }} }});
  try {{
    const result = await (async () => {{
{body}
    }})();
    await send({{ ok: true, result: result === undefined ? null : result }});
  }} catch (e) {{
    // WebKit's stack doesn't repeat the message, so send both.
    await send({{ ok: false, error: e && e.message ? e.message + "\n" + (e.stack || "") : String(e) }});
  }}
}})();"#
    )
}
