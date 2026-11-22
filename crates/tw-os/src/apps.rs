//! The frontmost application and its category.

pub use tw_core::text::AppCategory;

use super::{AppInfo, Result, platform};

/// The app that has keyboard focus: name, id (macOS bundle id, Windows exe
/// name, X11 `WM_CLASS`) and category. Needs no permission. `None` when no
/// app is focused, or on Wayland, which does not tell.
pub fn frontmost() -> Result<Option<AppInfo>> {
    platform::apps::frontmost()
}

/// Known apps by every id they go by on each OS, lowercased: macOS bundle
/// ids, Windows exe names without `.exe`, X11 `WM_CLASS` names.
const APPS: &[(AppCategory, &[&str])] = &[
    (
        AppCategory::PersonalMessaging,
        &[
            "com.apple.mobilesms",
            "com.apple.ichat",
            "net.whatsapp.whatsapp",
            "desktop.whatsapp",
            "whatsapp",
            "whatsapp.root",
            "ru.keepcoder.telegram",
            "org.telegram.desktop",
            "telegram",
            "telegram-desktop",
            "telegramdesktop",
            "org.whispersystems.signal-desktop",
            "signal",
            "signal desktop",
            "com.hnc.discord",
            "discord",
            "com.facebook.archon",
            "messenger",
            "com.tdesktop.telegram",
            "jp.naver.line.mac",
            "line",
            "com.viber.osx",
            "viber",
            "im.riot.app",
            "element",
            "com.skype.skype",
            "skype",
        ],
    ),
    (
        AppCategory::WorkMessaging,
        &[
            "com.tinyspeck.slackmacgap",
            "slack",
            "com.microsoft.teams",
            "com.microsoft.teams2",
            "ms-teams",
            "teams",
            "microsoft teams",
            "com.mattermost.desktop",
            "mattermost",
            "com.electron.zulip",
            "zulip",
            "chat.rocket",
            "rocket.chat",
            "us.zoom.xos",
            "zoom",
            "com.google.chat",
            "com.webex.meetingmanager",
            "cisco webex meetings",
        ],
    ),
    (
        AppCategory::Email,
        &[
            "com.apple.mail",
            "com.microsoft.outlook",
            "outlook",
            "olk",
            "com.readdle.smartemail-mac",
            "com.readdle.spark",
            "spark",
            "com.superhuman.electron",
            "superhuman",
            "com.mimestream.mimestream",
            "org.mozilla.thunderbird",
            "thunderbird",
            "com.airmailapp.airmail5",
            "com.freron.mailmate",
            "com.canarymail.mac",
            "it.bloop.airmail2",
            "com.google.gmail",
            "hey",
            "com.basecamp.hey",
            "evolution",
            "geary",
            "mailspring",
        ],
    ),
    (
        AppCategory::Code,
        &[
            "com.microsoft.vscode",
            "com.microsoft.vscodeinsiders",
            "code",
            "code - insiders",
            "com.todesktop.230313mzl4w4u92",
            "cursor",
            "dev.zed.zed",
            "zed",
            "com.apple.dt.xcode",
            "com.jetbrains.intellij",
            "com.jetbrains.pycharm",
            "com.jetbrains.webstorm",
            "com.jetbrains.goland",
            "com.jetbrains.rustrover",
            "com.jetbrains.clion",
            "idea64",
            "pycharm64",
            "webstorm64",
            "jetbrains-idea",
            "com.sublimetext.4",
            "sublime_text",
            "com.apple.terminal",
            "com.googlecode.iterm2",
            "dev.warp.warp-stable",
            "warp",
            "com.mitchellh.ghostty",
            "ghostty",
            "net.kovidgoyal.kitty",
            "kitty",
            "io.alacritty",
            "alacritty",
            "windowsterminal",
            "gnome-terminal-server",
            "konsole",
            "com.exafunction.windsurf",
            "windsurf",
        ],
    ),
    (
        AppCategory::Documents,
        &[
            "com.apple.iwork.pages",
            "com.apple.notes",
            "com.apple.textedit",
            "com.microsoft.word",
            "winword",
            "com.microsoft.powerpoint",
            "powerpnt",
            "com.microsoft.onenote.mac",
            "onenote",
            "notion.id",
            "notion",
            "md.obsidian",
            "obsidian",
            "com.lukilabs.lukiapp",
            "craft",
            "net.shinyfrog.bear",
            "com.ulyssesapp.mac",
            "com.agiletortoise.drafts-osx",
            "libreoffice",
            "soffice",
            "libreoffice-writer",
            "notepad",
            "org.gnome.texteditor",
            "gedit",
            "com.linear",
            "linear",
        ],
    ),
    (
        AppCategory::Browser,
        &[
            "com.apple.safari",
            "com.google.chrome",
            "chrome",
            "google-chrome",
            "org.mozilla.firefox",
            "firefox",
            "com.microsoft.edgemac",
            "msedge",
            "microsoft-edge",
            "company.thebrowser.browser",
            "arc",
            "com.brave.browser",
            "brave",
            "brave-browser",
            "com.operasoftware.opera",
            "opera",
            "com.vivaldi.vivaldi",
            "vivaldi",
            "vivaldi-stable",
            "app.zen-browser.zen",
            "zen",
            "chromium",
            "chromium-browser",
            "org.chromium.chromium",
        ],
    ),
];

/// The category of an app by its id, falling back to its display name.
pub fn categorize(id: Option<&str>, name: &str) -> AppCategory {
    let lookup = |key: &str| {
        let key = key.trim().to_lowercase();
        let key = key.strip_suffix(".exe").unwrap_or(&key);
        APPS.iter()
            .find(|(_, ids)| ids.contains(&key))
            .map(|(category, _)| *category)
    };
    id.and_then(lookup)
        .or_else(|| lookup(name))
        .unwrap_or_default()
}

pub(crate) fn app_info(name: String, id: Option<String>) -> AppInfo {
    let category = categorize(id.as_deref(), &name);
    AppInfo { name, id, category }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn categorizes_by_id_on_every_os() {
        assert_eq!(
            categorize(Some("com.apple.MobileSMS"), "Messages"),
            AppCategory::PersonalMessaging
        );
        assert_eq!(
            categorize(Some("com.tinyspeck.slackmacgap"), "Slack"),
            AppCategory::WorkMessaging
        );
        assert_eq!(
            categorize(Some("Slack.exe"), "Slack"),
            AppCategory::WorkMessaging
        );
        assert_eq!(
            categorize(Some("OUTLOOK.EXE"), "Outlook"),
            AppCategory::Email
        );
        assert_eq!(
            categorize(Some("com.mimestream.Mimestream"), "Mimestream"),
            AppCategory::Email
        );
        assert_eq!(
            categorize(Some("discord"), "Discord"),
            AppCategory::PersonalMessaging
        );
        assert_eq!(
            categorize(Some("com.microsoft.VSCode"), "Code"),
            AppCategory::Code
        );
        assert_eq!(
            categorize(Some("notion.id"), "Notion"),
            AppCategory::Documents
        );
        assert_eq!(categorize(Some("firefox"), "Firefox"), AppCategory::Browser);
    }

    #[test]
    fn falls_back_to_the_name_then_other() {
        assert_eq!(
            categorize(Some("com.example.unknown"), "Superhuman"),
            AppCategory::Email
        );
        assert_eq!(
            categorize(None, " Telegram "),
            AppCategory::PersonalMessaging
        );
        assert_eq!(
            categorize(Some("com.example.unknown"), "Unknown"),
            AppCategory::Other
        );
        assert_eq!(categorize(None, ""), AppCategory::Other);
    }

    #[test]
    fn every_id_belongs_to_one_category() {
        let mut seen = HashSet::new();
        for (_, ids) in APPS {
            for id in *ids {
                assert_eq!(*id, id.to_lowercase(), "{id} is lowercase");
                assert!(seen.insert(*id), "{id} is listed once");
            }
        }
    }
}
