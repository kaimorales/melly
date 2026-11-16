# Installation and updates

Melly has one installed identity: `/Applications/Melly.app`, bundle ID `com.melly.desktop`. Updating replaces the application bundle. The existing Application Support profile, managed project files, linked directories and Keychain records are outside that bundle and are never part of installer cleanup.

## Local development

Quit Melly, then run `npm run install:local`. It builds the current checkout, validates the production bundle identity and strict code signature, copies to a private staging directory next to the destination, verifies the staged copy, and replaces the previous app. A failed promotion restores the previous bundle. A successful installation removes its temporary backup and the checkout’s known build copy; it does not create a dated copy. If rollback itself fails, the error identifies the retained recovery directory.

The installer refuses a running app, a different bundle, a source/destination alias, and a Homebrew-owned installation. It never uses sudo, changes macOS security settings, or edits Keychain access rules. Ad-hoc signing is valid for local development; it is not a distribution signature or notarization.

It links `~/.local/bin/melly` to the installed binary without overwriting a conflicting command. Add that directory to PATH if needed. `npm run launch` prefers the installed app and otherwise opens the normal build artifact. `npm run build` alone remains a packaging command: if an installed app exists, finish with the installer rather than launching or saving another build copy. Temporary QA bundles must be removed after testing.

## Terminal commands

| Command | Behavior |
| --- | --- |
| `melly` or `melly --open` | Opens the app containing the launcher binary. |
| `melly --version` | Prints the app version without opening a window or profile. |
| `melly --help` | Prints command help. |
| `melly --update` / `melly --upgrade` | Requires Melly to be closed, checks installed Homebrew metadata, and runs `brew upgrade --cask kaimorales/tap/melly` only when that cask owns the installation. |

The commands run before Tauri and any profile or credential store is initialized. There is no update polling at startup. An unmanaged/source build reports that no update was installed and explains the local installation command. There is no direct-download self-updater and no silent switch of update channels. Homebrew failure exits nonzero. These commands are user-invoked app management, not tools available to models.

## Homebrew preparation

The intended distribution is a cask in `kaimorales/homebrew-tap`. Its app artifact installs `Melly.app`; its binary artifact creates the `melly` command. Once that tap and a real release exist, installation is `brew install --cask kaimorales/tap/melly`, and either update alias delegates to Homebrew. The tap and public update feed have **not** been published by this work.

`packaging/homebrew/melly.rb.in` is a template, not an installable public cask. Before a release, synchronize versions in package.json, src-tauri/Cargo.toml, and src-tauri/tauri.conf.json; build and verify a Developer ID signed, notarized Apple Silicon app. Archive it with top-level `Melly.app` as `Melly-VERSION-macos-arm64.zip`. The current template explicitly requires arm64 and macOS Monterey or later; it does not claim an Intel build.

Run `npm run prepare:homebrew -- /path/to/Melly-VERSION-macos-arm64.zip` to generate `target/homebrew/melly.rb` with the archive’s actual SHA-256. This only prepares metadata; it does not attest signing, publish the archive, create a tap, or submit to Homebrew. After separately authorized publication, verify the release URL/checksum and test cask installation, both update aliases and uninstall while retaining the profile. Each public update needs a new version and checksum. Do not add a `zap` action that deletes user data by default.

Sources: [Homebrew app and binary artifacts](https://docs.brew.sh/Cask-Cookbook), [Homebrew upgrade and installed-package metadata](https://docs.brew.sh/Manpage), [Tauri signing for macOS](https://v2.tauri.app/distribute/sign/macos/). Direct-download updates would be a separate future feature using [Tauri’s signed updater](https://v2.tauri.app/plugin/updater/), not unsigned download-and-replace code.

`CARGO_TARGET_DIR` is shared by the build and installer. Relative paths resolve from the checkout root so Tauri changing its working directory cannot select a different artifact.
