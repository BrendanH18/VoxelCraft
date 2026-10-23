# Updating VoxelCraft

[← Roadmap](roadmap.md) · [Documentation index](README.md) · [Release guide](releases.md)

**Status: implemented for v0.10, unverified.** The owner made easy upgrades
the top priority on 2026-10-22, ahead of new content, because releases will
be frequent. v0.10 is the stepping stone: everyone installs it by hand once,
and from then on the game updates itself. The code is in `src/update/`; the
[verification record](verification/2026-10-22-in-game-updater.md) lists what
has been checked. It tracks requirement
[UX-04b](requirements.md#ux--presentation-usability-and-distribution).

## Updating by hand (v0.9 and earlier)

Versions before v0.10 can't update themselves, so install v0.10 this way.

1. Quit VoxelCraft.
2. Back up each world you care about: copy its `saves/<name>` folder out of the
   [data folder](releases.md#player-data-and-old-saves). Newer versions upgrade
   worlds, and older versions can't open them again.
3. Download the new installer from
   [Releases](https://github.com/BrendanH18/VoxelCraft/releases).
4. **Windows:** run the new `Setup.exe`. It installs over the old version, since
   the installer's AppId never changes. **Mac:** open the `.dmg` and drag
   VoxelCraft onto the Applications shortcut, choosing **Replace**. Then repeat
   the [first-launch steps](releases.md#unsigned-first-launch).

Worlds, settings and logs live in the data folder, outside the app, so
installing a new version never touches them.

## How the game updates itself (v0.10 and later)

When a new version is out, the title screen shows an **Update to 0.11**
button in the bottom-right corner. It opens the update screen with the
release notes and four choices: **Update and Restart**, **Release Page**,
**Later** (hide it until the next launch) and **Skip This Version**.
Updating downloads the new version, checks it, backs up your worlds and
restarts into the new version. There is no account, no telemetry and no
background service.

1. **Check.** When the title screen opens, a background thread asks GitHub
   for the repository's latest published release
   (`GET https://api.github.com/repos/BrendanH18/VoxelCraft/releases/latest`).
   GitHub leaves drafts and pre-releases out, so publishing a draft is what
   releases an update. The check never delays startup and stays silent when
   offline; failures go to `voxelcraft.log`. **Check for Updates** in Options
   (from the next launch) and `--no-update-check` turn it off. Runs with
   `--data-dir`, screenshots and benchmarks don't check, unless
   `--update-source` names a test folder.
2. **Compare.** The tag is read as a version: `v0.10` and `0.10` mean `0.10.0`,
   and `v0.10.1` is a fix release. Only a version newer than the running one
   is offered, and not one the player skipped (`skip_update` in
   `options.txt`). Tags that aren't versions are ignored.
3. **Download.** The game downloads its platform's package
   (`VoxelCraft-<version>-windows-x64-Setup.exe` or
   `VoxelCraft-<version>-macos-apple-silicon.dmg`), `SHA256SUMS.txt` and
   `SHA256SUMS.txt.sig` into `<data-root>/updates/<version>/`, with a progress
   bar and **Cancel**.
4. **Verify.** The checksum list must carry an Ed25519 signature from a key in
   `packaging/update-keys.txt`, which is compiled into the game, and the
   package's SHA-256 must match its line in the list. Anything else stops the
   update.
5. **Back up.** The game holds each world's `session.lock` while copying it,
   so no other window can save it halfway through. If any world is open in
   another VoxelCraft window, the update stops and asks the player to close
   it. Worlds and `options.txt` go to
   `<data-root>/backups/<date>_<time>-before-<version>/` (UTC); the three
   newest pre-update backups are kept.
6. **Install and restart.**
   - **Windows:** the game starts the installer with
     `/SILENT /SUPPRESSMSGBOXES /NORESTART /UPDATE=1 /WAITPID=<game>` and exits.
     The installer waits for the game to close, upgrades the installation in
     place (Inno Setup rolls back an install that fails) and reopens the game.
   - **Mac:** the game mounts the disk image read-only, copies the new app to
     `.VoxelCraft-update.app` beside the installed one, and checks its code
     signature and version. It then starts a watcher (the game's own program,
     run with `--finish-update`, one at a time) and exits. The watcher swaps
     the two apps in one atomic step and opens the new one with the same data
     folder
     (`--data-dir` and `--update-source` only, never one-shot options such as
     `--new`). Once the new version has opened its window, it tells the
     watcher, which deletes the old app. If the new version doesn't start
     within two minutes, the watcher swaps the old app back and reopens it;
     the reopened game says the update didn't finish.
   - **Copies that can't replace themselves** show the notice, the reason and
     **Release Page** instead of **Update and Restart**: source builds, Linux,
     Intel Macs, a Windows copy not installed by the installer, and a Mac app
     run from the disk image or from a folder the player can't change.
7. **Report.** The next launch shows "Updated to VoxelCraft 0.11" on the title
   screen and removes the download, or, if the old version is still running,
   says the update didn't finish. Any failure before installing leaves the
   installed version untouched and shows the reason on the update screen,
   with **Try Again** and **Release Page**.

Known limits: on Windows, nothing rolls back a new version that installs but
then fails to start. On a Mac, if the watcher is stopped between the swap and
the new version starting, the old app stays hidden as `.VoxelCraft-update.app`
until the next update replaces it. In both cases the player can reinstall from
the release page, and the worlds are in the backup.

## Security

- Network access goes through the operating system's `curl`
  (`/usr/bin/curl`, or `curl.exe` in `System32` on Windows 10 and later), over
  HTTPS only, including redirects. Downloads must come from
  `https://github.com/BrendanH18/VoxelCraft/releases/download/`. The game
  bundles no TLS stack and uses the system's certificate store.
- The signature proves the release workflow produced the checksum list: the
  private key is only in the `UPDATE_SIGNING_KEY` GitHub Actions secret and the
  owner's offline copy. The checksum proves the package matches the list.
- No elevation. The Windows installer is per-user, and the Mac swap happens only
  where the player can already write.
- Files the game downloads itself don't receive the browser's Mark of the Web
  (Windows) or quarantine flag (Mac), so updates skip the unsigned first-launch
  prompt that a browser download triggers. That makes the signature and
  checksum checks the trust boundary, as they are for other self-updating
  apps. The game doesn't change any system security setting, and Windows
  policies such as Smart App Control can still block unsigned code.

## The signing key

Generated by the owner's agent on 2026-10-22 with
`openssl genpkey -algorithm ed25519`. The private key is at
`~/.config/voxelcraft/update-signing-key.pem` on the owner's Mac, outside the
repository.

- **It is in GitHub** as the `UPDATE_SIGNING_KEY` repository secret (set
  2026-10-22 with
  `gh secret set UPDATE_SIGNING_KEY --repo BrendanH18/VoxelCraft < ~/.config/voxelcraft/update-signing-key.pem`).
  Tag builds fail without it.
- **Keep an offline copy**, in a password manager for example. Every released
  game trusts only the keys listed in `packaging/update-keys.txt`. If the key
  is lost, players have to install the next version by hand.
- **To replace the key**, add the new public key to `packaging/update-keys.txt`
  and ship a release signed with the old key. Switch the secret to the new key
  for the release after that, then remove the old public key.
- `packaging/sign-checksums.sh` signs `SHA256SUMS.txt`. It refuses a key that
  isn't listed and checks the signature it writes.

## Release process

The same rules are in [the release guide](releases.md#updates).

- A published GitHub Release is an update offer to every player on v0.10 or
  later, so publish only when the release is meant for everyone. Drafts stay
  invisible.
- Keep asset names exactly as they are: the updater finds its package by
  name. The workflow attaches `SHA256SUMS.txt` and `SHA256SUMS.txt.sig`. Never
  replace published binaries.
- The release body is the in-game "what's new" text. Headings, lists, bold
  text and links are shown as plain text; long notes are cut off with a
  pointer to the release page.
- v0.10's release notes must tell players to install it by hand, once.

## Approaches considered

| Approach | Why not chosen |
|---|---|
| **In-house updater using the existing installers (chosen)** | Small; reuses the tested Inno Setup upgrade and the DMG; no new install layout or hosting |
| [Velopack](https://velopack.io) | A full installer and updater with a Rust crate and delta updates, but it replaces both installers with its own install layout and update packages. Worth reconsidering if delta downloads become important |
| Sparkle (Mac) and WinSparkle (Windows) | Two native frameworks to embed from Rust, an appcast feed to host, and different behavior on each platform |
| `self_update` crate | Replaces a single executable. That suits command-line tools, not a Mac app bundle or a Windows install with Start Menu entries and an uninstaller |
| A Rust HTTP client (`ureq` and `rustls`) | Adds a TLS stack and certificate bundle to the game; the system `curl` is already on every supported platform |
| Notify only, with a download link | Simple, but players still go through every manual step. It remains the fallback for copies that can't replace themselves |

## Testing

- Unit tests (`src/update/`) cover tag parsing and ordering; reading GitHub's
  release JSON and its notes; OpenSSL-made signatures, tampering and key
  rotation; checksum lists; backups (locks held, `session.lock` left out, a
  busy world refused, three kept, UTC names); the atomic Mac swap; the
  download-to-prepare path against a fixture signed with a test key; and the
  refusal of a fixture signed by any other key.
- `--update-source <folder>` reads a fixture release instead of GitHub: a
  `latest.json` in GitHub's format whose download URLs are file names in the
  folder. `--update-now` then installs an offered update without a click
  (once: a failed update turns it off).
- On 2026-10-22 a 0.9.0 Mac app updated itself to 0.9.1 this way, and a
  0.9.1 app rolled back from a 0.9.2 that crashed at launch
  ([record](verification/2026-10-22-in-game-updater.md)).
- The Windows release smoke test runs the installer's update mode: it waits
  for a stand-in game process, upgrades and runs the relaunch entry.
- Before publishing the first release after v0.10, the owner updates a real
  v0.10 install on each platform.

## Owner decisions (2026-10-22)

1. Check for updates by default; **Check for Updates** and `--no-update-check`
   turn it off.
2. Signed checksums, with the key in GitHub Actions.
3. Ask before every download.
4. Keep the last three pre-update backups.
5. No v0.9.1: v0.10 is the release everyone installs by hand.
