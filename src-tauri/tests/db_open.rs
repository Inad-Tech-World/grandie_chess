//! Opening a new database must not make the pool's connections fight over
//! locks: r2d2 logs each failed connection attempt as an error.

use std::sync::atomic::{AtomicUsize, Ordering};

use grandie_chess_lib::{db, training::puzzles};

static ERRORS: AtomicUsize = AtomicUsize::new(0);

struct CountErrors;

impl log::Log for CountErrors {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() == log::Level::Error
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{}: {}", record.target(), record.args());
            ERRORS.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn flush(&self) {}
}

#[test]
fn fresh_databases_open_without_lock_errors() {
    log::set_logger(&CountErrors).unwrap();
    log::set_max_level(log::LevelFilter::Error);
    let dir = tempfile::tempdir().unwrap();
    for i in 0..20 {
        db::open(&dir.path().join(format!("grandie-{i}.db"))).unwrap();
        puzzles::open(&dir.path().join(format!("puzzles-{i}.db"))).unwrap();
    }
    assert_eq!(ERRORS.load(Ordering::SeqCst), 0);
}
