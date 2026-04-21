use crate::cli::SessionCommand;
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use std::path::{Path, PathBuf};

fn firefox_profile_bases(home: &Path) -> [PathBuf; 3] {
    [
        home.join("snap/firefox/common/.mozilla/firefox"),
        home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
        home.join(".mozilla/firefox"),
    ]
}

fn is_visible_profile_dir(path: &Path) -> bool {
    path.is_dir()
        && path
            .file_name()
            .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
}

fn find_recovery_file(profile_dir: &Path) -> Option<PathBuf> {
    let recovery = profile_dir.join("sessionstore-backups/recovery.jsonlz4");
    recovery.exists().then_some(recovery)
}

fn recovery_files_in_base(base: &Path) -> Vec<PathBuf> {
    if !base.exists() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };

    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_visible_profile_dir(path))
        .filter_map(|path| find_recovery_file(&path))
        .collect()
}

fn newest_file(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .max_by_key(|path| path.metadata().ok().and_then(|meta| meta.modified().ok()))
}

fn session_recovery_candidates(home: &Path) -> Vec<PathBuf> {
    firefox_profile_bases(home)
        .into_iter()
        .flat_map(|base| recovery_files_in_base(&base))
        .collect()
}

fn latest_session_recovery(home: &Path) -> Option<PathBuf> {
    newest_file(session_recovery_candidates(home))
}

fn home_session_recovery() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    latest_session_recovery(&home)
}

fn find_session_file() -> Option<PathBuf> {
    home_session_recovery()
}

fn load_session(path: &PathBuf) -> Result<serde_json::Value> {
    use std::io::Read as IoRead;

    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;

    let mut compressed = Vec::new();
    file.read_to_end(&mut compressed)?;

    let decompressed = lz4_flex::decompress_size_prepended(&compressed)
        .map_err(|error| anyhow!("LZ4 decompression failed: {}", error))?;

    let session: serde_json::Value = serde_json::from_slice(&decompressed)?;
    Ok(session)
}

#[derive(Serialize)]
struct SessionTab {
    window: usize,
    title: String,
    url: String,
}

fn session_windows(session: &serde_json::Value) -> &[serde_json::Value] {
    session
        .get("windows")
        .and_then(|windows| windows.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn window_tabs(window: &serde_json::Value) -> &[serde_json::Value] {
    window
        .get("tabs")
        .and_then(|tabs| tabs.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn active_tab_entry(tab: &serde_json::Value) -> Option<&serde_json::Value> {
    let entries = tab.get("entries").and_then(|value| value.as_array())?;
    let index = tab
        .get("index")
        .and_then(|value| value.as_u64())
        .unwrap_or(1) as usize;
    if index == 0 || index > entries.len() {
        return None;
    }
    entries.get(index - 1)
}

fn session_tab_from_entry(entry: &serde_json::Value, window: usize) -> SessionTab {
    SessionTab {
        window,
        title: entry
            .get("title")
            .and_then(|title| title.as_str())
            .unwrap_or("No title")
            .to_string(),
        url: entry
            .get("url")
            .and_then(|url| url.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

fn collect_window_session_tabs(window: &serde_json::Value, window_index: usize) -> Vec<SessionTab> {
    window_tabs(window)
        .iter()
        .filter_map(|tab| active_tab_entry(tab))
        .map(|entry| session_tab_from_entry(entry, window_index + 1))
        .collect()
}

fn extend_session_tabs_from_window(
    tabs: &mut Vec<SessionTab>,
    window: &serde_json::Value,
    window_index: usize,
) {
    tabs.extend(collect_window_session_tabs(window, window_index));
}

fn get_session_tabs(session: &serde_json::Value) -> Vec<SessionTab> {
    let mut tabs = Vec::new();
    for (window_index, window) in session_windows(session).iter().enumerate() {
        extend_session_tabs_from_window(&mut tabs, window, window_index);
    }
    tabs
}

fn load_session_tabs() -> Result<Vec<SessionTab>> {
    let session_file = find_session_file().context("No Firefox session file found")?;
    let session = load_session(&session_file)?;
    Ok(get_session_tabs(&session))
}

fn filter_session_tabs(
    mut tabs: Vec<SessionTab>,
    search: Option<&str>,
    limit: Option<usize>,
) -> Vec<SessionTab> {
    if let Some(term) = search {
        let term_lower = term.to_lowercase();
        tabs.retain(|tab| {
            tab.title.to_lowercase().contains(&term_lower)
                || tab.url.to_lowercase().contains(&term_lower)
        });
    }

    if let Some(max_tabs) = limit {
        tabs.truncate(max_tabs);
    }

    tabs
}

fn print_default_session_tabs(tabs: &[SessionTab]) {
    for (index, tab) in tabs.iter().enumerate() {
        let title: String = tab.title.chars().take(70).collect();
        println!("{:4}. {}", index + 1, title);
        println!("      {}", tab.url);
    }
}

fn print_session_tabs(
    tabs: &[SessionTab],
    json: bool,
    urls_only: bool,
    titles_only: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(tabs)?);
        return Ok(());
    }

    if urls_only {
        for tab in tabs {
            println!("{}", tab.url);
        }
        return Ok(());
    }

    if titles_only {
        for tab in tabs {
            println!("{}", tab.title);
        }
        return Ok(());
    }

    print_default_session_tabs(tabs);
    Ok(())
}

fn handle_session_list(
    tabs: Vec<SessionTab>,
    json: bool,
    urls_only: bool,
    titles_only: bool,
    search: Option<String>,
    limit: Option<usize>,
) -> Result<()> {
    let filtered_tabs = filter_session_tabs(tabs, search.as_deref(), limit);
    print_session_tabs(&filtered_tabs, json, urls_only, titles_only)
}

fn session_tab_count(tabs: &[SessionTab]) -> usize {
    tabs.len()
}

fn handle_session_count(tabs: &[SessionTab]) {
    println!("{}", session_tab_count(tabs));
}

enum SessionAction {
    List {
        urls_only: bool,
        titles_only: bool,
        search: Option<String>,
        limit: Option<usize>,
    },
    Count,
}

fn classify_session_command(action: SessionCommand) -> SessionAction {
    match action {
        SessionCommand::List {
            urls_only,
            titles_only,
            search,
            limit,
        } => SessionAction::List {
            urls_only,
            titles_only,
            search,
            limit,
        },
        SessionCommand::Count => SessionAction::Count,
    }
}

fn execute_session_action(action: SessionAction, tabs: Vec<SessionTab>, json: bool) -> Result<()> {
    match action {
        SessionAction::List {
            urls_only,
            titles_only,
            search,
            limit,
        } => handle_session_list(tabs, json, urls_only, titles_only, search, limit),
        SessionAction::Count => {
            handle_session_count(&tabs);
            Ok(())
        }
    }
}

fn execute_session_command(
    action: SessionCommand,
    tabs: Vec<SessionTab>,
    json: bool,
) -> Result<()> {
    let session_action = classify_session_command(action);
    execute_session_action(session_action, tabs, json)
}

pub fn handle_session(action: SessionCommand, json: bool) -> Result<()> {
    let tabs = load_session_tabs()?;
    execute_session_command(action, tabs, json)
}

#[cfg(test)]
mod tests {
    use crate::cli::SessionCommand;

    use super::{
        SessionAction, SessionTab, classify_session_command, collect_window_session_tabs,
        extend_session_tabs_from_window, filter_session_tabs, get_session_tabs,
        latest_session_recovery, session_tab_count,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn get_session_tabs_selects_active_entry_per_tab() {
        let session = serde_json::json!({
            "windows": [
                {
                    "tabs": [
                        {
                            "index": 2,
                            "entries": [
                                {"title": "Old", "url": "https://old.example"},
                                {"title": "Current", "url": "https://current.example"}
                            ]
                        }
                    ]
                },
                {
                    "tabs": [
                        {
                            "entries": [
                                {"title": "Default Index", "url": "https://default.example"}
                            ]
                        }
                    ]
                }
            ]
        });

        let tabs = get_session_tabs(&session);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].window, 1);
        assert_eq!(tabs[0].title, "Current");
        assert_eq!(tabs[0].url, "https://current.example");
        assert_eq!(tabs[1].window, 2);
        assert_eq!(tabs[1].title, "Default Index");
        assert_eq!(tabs[1].url, "https://default.example");
    }

    #[test]
    fn get_session_tabs_skips_invalid_indices_and_applies_defaults() {
        let session = serde_json::json!({
            "windows": [
                {
                    "tabs": [
                        {
                            "index": 0,
                            "entries": [{"title": "Ignored", "url": "https://ignored.example"}]
                        },
                        {
                            "index": 3,
                            "entries": [{"title": "Too High", "url": "https://high.example"}]
                        },
                        {
                            "index": 1,
                            "entries": [{}]
                        }
                    ]
                }
            ]
        });

        let tabs = get_session_tabs(&session);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].window, 1);
        assert_eq!(tabs[0].title, "No title");
        assert_eq!(tabs[0].url, "");
    }

    #[test]
    fn filter_session_tabs_applies_case_insensitive_search_and_limit() {
        let tabs = vec![
            SessionTab {
                window: 1,
                title: "Rust Book".to_string(),
                url: "https://doc.rust-lang.org".to_string(),
            },
            SessionTab {
                window: 1,
                title: "Mozilla".to_string(),
                url: "https://www.mozilla.org".to_string(),
            },
            SessionTab {
                window: 2,
                title: "Example".to_string(),
                url: "https://example.com".to_string(),
            },
        ];

        let filtered = filter_session_tabs(tabs, Some("MOZ"), Some(1));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Mozilla");
    }

    #[test]
    fn firefox_profile_bases_builds_expected_paths() {
        let home = Path::new("/tmp/firefox-cli-home");
        let bases = super::firefox_profile_bases(home);
        assert_eq!(
            bases[0],
            PathBuf::from("/tmp/firefox-cli-home/snap/firefox/common/.mozilla/firefox")
        );
        assert_eq!(
            bases[1],
            PathBuf::from("/tmp/firefox-cli-home/.var/app/org.mozilla.firefox/.mozilla/firefox")
        );
        assert_eq!(
            bases[2],
            PathBuf::from("/tmp/firefox-cli-home/.mozilla/firefox")
        );
    }

    #[test]
    fn collect_window_session_tabs_uses_window_number_offset() {
        let window = serde_json::json!({
            "tabs": [
                {
                    "entries": [{"title": "Tab A", "url": "https://a.example"}]
                }
            ]
        });

        let tabs = collect_window_session_tabs(&window, 2);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].window, 3);
        assert_eq!(tabs[0].title, "Tab A");
    }

    #[test]
    fn extend_session_tabs_from_window_appends_tabs() {
        let window = serde_json::json!({
            "tabs": [
                {
                    "entries": [{"title": "Tab B", "url": "https://b.example"}]
                }
            ]
        });
        let mut tabs = vec![SessionTab {
            window: 1,
            title: "Existing".to_string(),
            url: "https://existing.example".to_string(),
        }];

        extend_session_tabs_from_window(&mut tabs, &window, 1);
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[1].window, 2);
        assert_eq!(tabs[1].title, "Tab B");
    }

    #[test]
    fn latest_session_recovery_prefers_newest_file() {
        use std::fs;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let home = std::env::temp_dir().join(format!("firefox-cli-session-test-{}", unique));
        let first = home.join(".mozilla/firefox/profile-a/sessionstore-backups/recovery.jsonlz4");
        let second = home.join(".mozilla/firefox/profile-b/sessionstore-backups/recovery.jsonlz4");

        fs::create_dir_all(first.parent().expect("first parent")).expect("create first dir");
        fs::write(&first, b"first").expect("write first");
        std::thread::sleep(Duration::from_millis(10));
        fs::create_dir_all(second.parent().expect("second parent")).expect("create second dir");
        fs::write(&second, b"second").expect("write second");

        let latest = latest_session_recovery(&home).expect("latest session file");
        assert_eq!(latest, second);

        fs::remove_dir_all(&home).expect("cleanup temp test dir");
    }

    #[test]
    fn latest_session_recovery_returns_none_for_missing_home() {
        let home = std::env::temp_dir().join("firefox-cli-missing-home-no-profiles");
        if home.exists() {
            std::fs::remove_dir_all(&home).expect("cleanup stale test dir");
        }
        assert!(latest_session_recovery(&home).is_none());
    }

    #[test]
    fn session_tab_count_matches_collection_size() {
        let tabs = vec![
            SessionTab {
                window: 1,
                title: "One".to_string(),
                url: "https://one.example".to_string(),
            },
            SessionTab {
                window: 2,
                title: "Two".to_string(),
                url: "https://two.example".to_string(),
            },
        ];
        assert_eq!(session_tab_count(&tabs), 2);
    }

    #[test]
    fn classify_session_command_maps_count_variant() {
        let action = classify_session_command(SessionCommand::Count);
        match action {
            SessionAction::Count => {}
            _ => panic!("expected count action"),
        }
    }
}
