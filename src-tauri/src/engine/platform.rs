//! What differs between the macOS and Windows builds of lc0, and how engine
//! processes are started on each.
//!
//! macOS bundles lc0 built from source with Metal (GPU) and Accelerate (CPU,
//! the `blas` backend). Windows bundles the official `onnx-dml` build, whose
//! GPU backend runs on any DirectX 12 card through DirectML and whose CPU
//! backend is ONNX Runtime.

use std::path::Path;

use tokio::process::Command;

/// lc0's GPU backend. It takes `gpu=N` backend options to pick a device.
#[cfg(not(windows))]
pub const GPU_BACKEND: &str = "metal";
#[cfg(windows)]
pub const GPU_BACKEND: &str = "onnx-dml";

/// lc0's CPU backend: instant for the one-node and few-node opponent levels.
#[cfg(not(windows))]
pub const CPU_BACKEND: &str = "blas";
#[cfg(windows)]
pub const CPU_BACKEND: &str = "onnx-cpu";

/// Backend options for benchmarking the CPU backend. ONNX Runtime uses one
/// thread unless told otherwise; Accelerate picks its own thread count.
pub fn cpu_benchmark_opts() -> Option<String> {
    if cfg!(windows) {
        let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(1).max(1)).unwrap_or(1);
        Some(format!("threads={threads}"))
    } else {
        None
    }
}

/// A command for a console helper program (an engine, or an archive tool).
/// On Windows it is started without a console window: Grandie is a GUI app,
/// so each engine would otherwise flash a terminal on screen.
pub fn engine_command(program: impl AsRef<Path>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program.as_ref());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
