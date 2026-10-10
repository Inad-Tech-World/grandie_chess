//! Regenerates `src/bindings.ts` so the UI types always match the Rust
//! commands, even without launching the app.

#[test]
fn export_typescript_bindings() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");
    grandie_chess_lib::export_bindings(path);
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("playStart"));
    assert!(text.contains("playUpdate"));
}
