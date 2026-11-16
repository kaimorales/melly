//! User-invoked app management. No model tool or webview command reaches this module.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{path::Path, process::Command};

const CASK: &str = "kaimorales/tap/melly";
const HELP: &str = "Melly — local and cloud conversations\n\nUsage: melly [option]\n\n  --open                 Open the installed app\n  --update, --upgrade     Upgrade a Homebrew-managed installation\n  --version              Print the version\n  --help                 Show this help\n\nUpdates replace Melly.app and retain chats, project files and Keychain credentials.\nHomebrew updates require a published cask and an installation managed by that cask.\nLocal development: run npm run install:local from the Melly source checkout.\n";

#[derive(Debug, PartialEq)]
enum Action {
    Gui,
    Open,
    Help,
    Version,
    Upgrade,
}

fn action(program: &str, arguments: &[String]) -> Result<Action> {
    match arguments {
        [] => Ok(
            if Path::new(program).file_name().is_some_and(|n| n == "melly") {
                Action::Open
            } else {
                Action::Gui
            },
        ),
        [arg] => match arg.as_str() {
            "--open" => Ok(Action::Open),
            "--help" | "-h" => Ok(Action::Help),
            "--version" | "-V" => Ok(Action::Version),
            "--update" | "--upgrade" => Ok(Action::Upgrade),
            value if value.starts_with("-psn_") => Ok(Action::Gui),
            _ => bail!("Unknown option. Run melly --help."),
        },
        _ => bail!("Choose one option. Run melly --help."),
    }
}

fn managed_by_homebrew(inventory: &Value) -> bool {
    inventory["casks"].as_array().is_some_and(|casks| {
        casks.iter().any(|cask| {
            cask["token"] == "melly"
                && cask["tap"] == "kaimorales/tap"
                && cask["installed"].as_str().is_some_and(|v| !v.is_empty())
        })
    })
}

fn upgrade_checked(
    running: bool,
    inventory: impl FnOnce() -> Result<Value>,
    upgrade: impl FnOnce() -> Result<i32>,
) -> Result<i32> {
    ensure!(
        !running,
        "Quit Melly before updating, then run melly --update again."
    );
    ensure!(
        managed_by_homebrew(&inventory()?),
        "This copy is not managed by the Melly Homebrew cask. No update was installed. For this source build, run npm run install:local from the Melly checkout."
    );
    upgrade()
}

#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
fn app_running() -> bool {
    use cocoa::{
        base::{id, nil},
        foundation::{NSAutoreleasePool, NSString},
    };
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool = NSAutoreleasePool::new(nil);
        let identifier = NSString::alloc(nil).init_str("com.melly.desktop");
        let apps: id = msg_send![class!(NSRunningApplication), runningApplicationsWithBundleIdentifier:identifier];
        let count: usize = msg_send![apps, count];
        let running = (0..count).any(|index| {
            let app: id = msg_send![apps, objectAtIndex:index];
            let pid: i32 = msg_send![app, processIdentifier];
            pid > 0 && pid as u32 != std::process::id()
        });
        let _: () = msg_send![identifier, release];
        pool.drain();
        running
    }
}

#[cfg(not(target_os = "macos"))]
fn app_running() -> bool {
    false
}

fn upgrade() -> Result<i32> {
    let brew = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .into_iter().find(|path| Path::new(path).is_file())
        .context("Homebrew is not installed. This build has no direct-download updater; run npm run install:local from the source checkout.")?;
    upgrade_checked(
        app_running(),
        || {
            let output = Command::new(brew)
                .args(["info", "--json=v2", "--installed"])
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env("HOMEBREW_NO_ANALYTICS", "1")
                .output()
                .context("Could not read Homebrew's installed packages")?;
            ensure!(
                output.status.success() && output.stdout.len() <= 8 * 1024 * 1024,
                "Could not read Homebrew's installed packages. No update was attempted."
            );
            serde_json::from_slice(&output.stdout)
                .context("Homebrew returned invalid package metadata")
        },
        || {
            println!("Updating Melly through Homebrew…");
            let status = Command::new(brew)
                .args(["upgrade", "--cask", CASK])
                .status()
                .context("Could not start Homebrew")?;
            if status.success() {
                println!("Melly is up to date. Run melly to open it.");
            }
            Ok(status.code().unwrap_or(1))
        },
    )
}

fn open_app() -> Result<i32> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let bundle = executable
        .ancestors()
        .nth(3)
        .context("Melly app bundle not found")?;
    ensure!(
        bundle.file_name().is_some_and(|name| name == "Melly.app"),
        "Install Melly.app before using the launcher."
    );
    let status = Command::new("/usr/bin/open").arg(bundle).status()?;
    Ok(status.code().unwrap_or(1))
}

/// Called before Tauri, the profile database or any credential store is opened.
pub fn run() -> Option<i32> {
    let mut args = std::env::args();
    let program = args.next().unwrap_or_default();
    let result = action(&program, &args.collect::<Vec<_>>()).and_then(|action| match action {
        Action::Gui => Ok(None),
        Action::Open => open_app().map(Some),
        Action::Help => {
            print!("{HELP}");
            Ok(Some(0))
        }
        Action::Version => {
            println!("Melly {}", env!("CARGO_PKG_VERSION"));
            Ok(Some(0))
        }
        Action::Upgrade => upgrade().map(Some),
    });
    match result {
        Ok(code) => code,
        Err(reason) => {
            eprintln!("{reason}");
            Some(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cli_aliases_do_not_enter_the_gui_and_extra_arguments_fail() {
        for option in ["--update", "--upgrade"] {
            assert_eq!(action("melly", &[option.into()]).unwrap(), Action::Upgrade);
        }
        assert_eq!(
            action("/opt/homebrew/bin/melly", &[]).unwrap(),
            Action::Open
        );
        assert_eq!(action("melly-desktop", &[]).unwrap(), Action::Gui);
        assert!(action("melly", &["--upgrade".into(), "other-package".into()]).is_err());
        assert!(action("melly", &["--unknown".into()]).is_err());
    }

    #[test]
    fn running_app_prevents_even_homebrew_inventory_access() {
        assert!(
            upgrade_checked(
                true,
                || panic!("must not query"),
                || panic!("must not upgrade")
            )
            .is_err()
        );
    }

    #[test]
    fn missing_or_different_casks_never_trigger_an_install() {
        for value in [
            json!({"casks":[]}),
            json!({"casks":[{"token":"melly","tap":"another/tap","installed":"1.0"}]}),
            json!({"casks":[{"token":"melly","tap":"kaimorales/tap","installed":null}]}),
        ] {
            assert!(upgrade_checked(false, || Ok(value), || panic!("must not upgrade")).is_err());
        }
    }

    #[test]
    fn upgrade_preserves_homebrew_failure_status() {
        let inventory =
            json!({"casks":[{"token":"melly","tap":"kaimorales/tap","installed":"1.0.0"}]});
        assert_eq!(
            upgrade_checked(false, || Ok(inventory), || Ok(7)).unwrap(),
            7
        );
    }
}
