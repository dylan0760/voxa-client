# Voxa (client)

Voxa is free, open source voice chat with persistent encrypted text chat and screen
sharing. It is written in Rust for Windows and has a TeamSpeak-6-style interface. There
are no license keys, accounts, registrations or fees.

This repository is the **desktop client**. The server lives at
[dylan0760/voxa-server](https://github.com/dylan0760/voxa-server).

Voxa is an independent clean-room implementation. It contains no TeamSpeak code or
assets and does not use the TeamSpeak protocol.

## Download
Get `Voxa-<version>-x64.msi` from the
[latest release](https://github.com/dylan0760/voxa-client/releases/latest) and run it.
The MSI is not code-signed. If Windows SmartScreen warns you, choose
**More info → Run anyway**.

## Features
* **TeamSpeak 6 style layout:**
  * Server rail on the far left.
  * Channel tree with avatars and speaking rings.
  * "Voice Connected" card and your user panel (mute microphone, mute speakers,
    settings).
  * Chat in the main area.
  * Info panel on the right.
* **Voice:** Opus (pure Rust) with voice activation, push-to-talk (any key or mouse
  button) or continuous transmission. Per-user volume, local mute and notification
  sounds.
* **Chat:** channel and server chat is stored encrypted on the server, so the history
  follows you to any PC (like Discord). Private messages are also supported.
* **Emotes:** full-colour emoji in chat.
  * Use the 😀 picker (search, categories, recently used).
  * Or type `:shortcode:`, e.g. `:fire:` `:+1:` `:joy:`; suggestions appear as you type
    and Tab inserts the first one.
  * Emoji-only messages are shown jumbo-sized.
* **Drag & drop:** drag channels to reorder them.
  * Drop on the top or bottom edge of a channel to place it above or below.
  * Drop on the middle to make it a sub-channel.
  * Drag users onto a channel to move them.
* **Hotkeys:** global hotkeys under **Settings → Hotkeys**. They work in the background
  and from the tray.
  * Toggle microphone or speaker mute, push-to-mute, toggle away, channel commander,
    start/stop screen sharing, bring Voxa to front.
  * Push-to-talk is set under **Capture**.
* **Join power:** channels can require a minimum group *join power* (TeamSpeak style).
  Channels you can't enter show ⛔.
* **Screen sharing:** share a whole screen or a single application. Streams are relayed
  through the server, never peer-to-peer. Viewers watch on a stage above the chat or in
  full view.
* **File transfers:**
  * Uploads and downloads show progress, with **Cancel** and **Retry**.
  * Stalled or interrupted transfers are detected automatically, and the file
    connection reconnects by itself.
  * Admins set the max file size and per-channel storage (or unlimited) under
    **Edit Virtual Server ▸ File transfers**.
* **Server tools:** file browser per channel, permissions (server and channel groups),
  privilege keys, bans, kick, move, poke, talk power, channel commander.
* **Everyday conveniences:** identities (export/import), bookmarks, `voxa://host:port`
  links, and trust-on-first-use server fingerprint pinning.
* **System tray:** closing the window (✕) keeps Voxa running in the notification area.
  Toggle it in **Settings → Application → System tray**.
* **Updates:** Voxa checks this repository's GitHub releases at startup and every 12
  hours. Turn this off in **Settings → Application → Updates**. Click the green ⬆
  button, or open **About → Check for updates**, to download and install the new
  version. Voxa closes, upgrades and restarts automatically. The download is checked
  against GitHub's SHA-256 digest.

## Network
The client connects to the server on **TCP and UDP 9987** (or the port in the address).
No inbound ports are needed on the client side.

## Security
* **Connections:** TCP uses Noise `XX_25519_ChaChaPoly_BLAKE2s` with mutual
  authentication. Your identity key defines your unique ID.
* **Voice:** ChaCha20-Poly1305 with a per-session key and replay protection.

## Building
Requirements: Rust (rustup) and PowerShell.

```powershell
.\build.ps1
```

The script installs the `stable-x86_64-pc-windows-gnullvm` toolchain if needed and
downloads portable LLVM-MinGW and WiX 3.14 into `%LOCALAPPDATA%\voxa-build-tools`. It
writes the installer to `dist\`. No C compiler or Visual Studio is required.

For development: `cargo run -p voxa-client`.

## Releasing an update
1. Bump `version` in `Cargo.toml` (for example `1.2.0`) and commit.
2. Push a matching tag: `git tag v1.2.0` then `git push origin v1.2.0`.
3. GitHub Actions builds the MSI and publishes the release. Installed clients offer the
   update automatically.

To point a fork's updater at another repository, build with
`$env:VOXA_UPDATE_REPO = "you/your-fork"`.

## Protocol crate
`crates/voxa-proto` (wire protocol, crypto and updater) is shared with
[voxa-server](https://github.com/dylan0760/voxa-server). Keep both copies identical,
and bump `PROTOCOL_VERSION` for incompatible changes.

## License
Dual-licensed under MIT or Apache-2.0, at your option. Third-party licenses are listed
in `THIRD-PARTY-NOTICES.txt` in the installed program folder.
