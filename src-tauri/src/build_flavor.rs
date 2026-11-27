//! Development builds run next to the installed app and share nothing with it
//! but the model files (`docs/operations/development.md`).

use tauri::{Context, Runtime};
use tw_core::Settings;
use tw_core::hotkey::{Platform, Shortcuts};

/// Debug builds, except under `cargo test`, whose tests expect release
/// behaviour.
pub const DEVELOPMENT: bool = cfg!(all(debug_assertions, not(test)));

const RELEASE_NAME: &str = "Tiny Wispr";
const DEVELOPMENT_NAME: &str = "Tiny Wispr Dev";
const IDENTIFIER_SUFFIX: &str = ".dev";

pub const APP_NAME: &str = if DEVELOPMENT {
    DEVELOPMENT_NAME
} else {
    RELEASE_NAME
};

/// The identifier picks the data folder, the single-instance lock and the
/// login item, so a development build gets its own. `pnpm app:mac` bundles
/// with `tauri.dev.conf.json`, which sets the same identifier for macOS.
pub fn apply<R: Runtime>(context: &mut Context<R>) {
    if !DEVELOPMENT {
        return;
    }
    let config = context.config_mut();
    config.identifier = development_identifier(&config.identifier);
    config.product_name = Some(DEVELOPMENT_NAME.to_owned());
    context.package_info_mut().name = DEVELOPMENT_NAME.to_owned();
}

fn development_identifier(identifier: &str) -> String {
    if identifier.ends_with(IDENTIFIER_SUFFIX) {
        identifier.to_owned()
    } else {
        format!("{identifier}{IDENTIFIER_SUFFIX}")
    }
}

/// The installed app's identifier, whose model folder a development build
/// uses.
pub fn release_identifier(identifier: &str) -> &str {
    identifier
        .strip_suffix(IDENTIFIER_SUFFIX)
        .unwrap_or(identifier)
}

pub fn default_shortcuts() -> Shortcuts {
    if DEVELOPMENT {
        Shortcuts::development(Platform::current())
    } else {
        Shortcuts::defaults(Platform::current())
    }
}

pub fn default_settings() -> Settings {
    Settings {
        shortcuts: default_shortcuts(),
        ..Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_round_trip() {
        let dev = development_identifier("dev.time2code.tinywispr");
        assert_eq!(dev, "dev.time2code.tinywispr.dev");
        assert_eq!(development_identifier(&dev), dev);
        assert_eq!(release_identifier(&dev), "dev.time2code.tinywispr");
        assert_eq!(
            release_identifier("dev.time2code.tinywispr"),
            "dev.time2code.tinywispr"
        );
    }

    #[test]
    fn the_bundle_overlay_matches() {
        let read = |name: &str| -> serde_json::Value {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
        };
        let release = read("tauri.conf.json");
        let dev = read("tauri.dev.conf.json");
        assert_eq!(
            dev["identifier"].as_str().unwrap(),
            development_identifier(release["identifier"].as_str().unwrap())
        );
        assert_eq!(dev["productName"], DEVELOPMENT_NAME);
        assert_eq!(release["productName"], RELEASE_NAME);
    }
}
