//! The app's version lives in four files: the Cargo workspace (this crate's version),
//! `src-tauri/tauri.conf.json` (what the bundles, the updater and `latest.json` use),
//! `package.json` and `package-lock.json`. A release bumps them together; this test fails CI
//! when one is left behind.

use serde_json::Value;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn json(path: &str) -> Value {
    let full = format!("{ROOT}/{path}");
    let text = std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{full}: {e}"))
}

#[test]
fn every_version_matches_the_cargo_workspace() {
    let cargo = env!("CARGO_PKG_VERSION");
    let lock = json("package-lock.json");
    let found = [
        (
            "src-tauri/tauri.conf.json",
            json("src-tauri/tauri.conf.json")["version"].clone(),
        ),
        ("package.json", json("package.json")["version"].clone()),
        ("package-lock.json", lock["version"].clone()),
        (
            "package-lock.json packages[\"\"]",
            lock["packages"][""]["version"].clone(),
        ),
    ];
    for (file, version) in found {
        assert_eq!(
            version.as_str(),
            Some(cargo),
            "{file} has version {version}, the Cargo workspace has {cargo}"
        );
    }
}
