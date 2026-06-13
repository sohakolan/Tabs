//! Lancement au démarrage via un LaunchAgent utilisateur.
//!
//! On écrit (ou supprime) `~/Library/LaunchAgents/fr.myrole.tabs.plist`.
//! `launchd` lance alors l'exécutable à l'ouverture de session (`RunAtLoad`).

use std::fs;
use std::path::PathBuf;

const LABEL: &str = "fr.myrole.tabs";

/// Active ou désactive le lancement de Tabs à l'ouverture de session.
pub fn set_launch_at_login(enabled: bool) {
    let Some(path) = plist_path() else {
        return;
    };
    if enabled {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&path, plist_contents(&exe.to_string_lossy()));
    } else {
        let _ = fs::remove_file(&path);
    }
}

fn plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    path.push(format!("Library/LaunchAgents/{LABEL}.plist"));
    Some(path)
}

fn plist_contents(program: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{program}</string>
		<string>--login</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
</dict>
</plist>
"#
    )
}
