//! UCI engines: process control, protocol parsing and lc0 configuration.

pub mod benchmark;
pub mod catalog;
pub mod lc0;
pub mod platform;
pub mod process;
pub mod registry;
pub mod uci;

pub mod install;

use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use process::Engine;

/// Start a UCI engine, send its options and wait until it is ready. lc0 loads
/// its network during `isready`, hence the long timeout.
pub async fn launch(path: &Path, options: &[(String, String)]) -> Result<Arc<Engine>> {
    let engine = Engine::spawn(path, &[]).await?;
    for (name, value) in options {
        engine.set_option(name, value).await?;
    }
    engine.ready(lc0::READY_TIMEOUT).await?;
    Ok(engine)
}
