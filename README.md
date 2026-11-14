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

Servers running Voxa Server 1.10.0 or newer normally require the newest Voxa. If a
server needs a newer version, Voxa tells you and opens the update window.

## Features

### Layout
* **TeamSpeak 6 style:**
  * Server rail on the far left. Drag your saved servers to put them in any order.
  * Channel tree with avatars and speaking rings.
  * "Voice Connected" card and your user panel (mute microphone, mute speakers,
    settings).
  * Chat in the main area.
  * Info panel on the right.
* **Themes:** Dark, Light, 🌸 Sakura (cherry blossom), 🍼 Layette (baby blue),
  🍷 Crimson, 🌌 Midnight (purple) and 🍵 Celadon (jade green). Pick one in
  **Options ▸ Application ▸ Theme**.
* **Avatars** (Voxa Server 1.15.0+): click your name ▸ **Change Avatar…**.
  * Keep your initials and pick their **colour**, or use **your own picture**.
  * Or pick one of 60 **icons** (headphones, speakers, microphones, controllers, tools,
    gear, robots, animals and more), drawn in colour on the colour you choose. Handy for
    giving bots and tools their own picture.
  * Pictures are cropped like on Steam: drag the square to move it, drag a corner or
    scroll to resize it, with live round previews at every size Voxa uses.
  * The avatar belongs to the identity, so every server shows it. Another PC using the
    same identity picks it up automatically.
  * Moderators (*Edit client descriptions*) can remove an offensive picture; it can't be
    used on that server again.
* **Windows:** dialogs can be moved and resized.
* **Frame rate limit:** set it under **Options ▸ Application** (60 fps by default) to
  save GPU on fast monitors.

### Voice
* **Codecs**, chosen per channel:

  | Codec | Sound | Bitrate |
  |---|---|---|
  | **Opus Voice** | Mono, tuned for speech | 8–96 kbit/s |
  | **Opus Music** | Stereo, full band (Voxa Server 1.10.0+) | 32–192 kbit/s |
  | **Lossless** | 16-bit 48 kHz stereo, bit exact, compressed like FLAC (Voxa Server 1.11.0+) | about 0.8–1.5 Mbit/s per listener |

* **Transmission:** voice activation, push-to-talk (any key or mouse button) or
  continuous.
* **Capture from** (**Options ▸ Capture**):
  * your **microphone**;
  * **system sound**: everything your PC plays except Voxa itself, so nobody hears the
    voice chat echoed;
  * **one application**, for example your music player.

  To play music to a channel, use Opus Music or Lossless with Continuous Transmission.
* **Microphone test:** **Options ▸ Capture ▸ Record a test** records up to 15 seconds
  and plays it back. It works without a server, and nothing is transmitted meanwhile.
* **Muted warning:** talking while your microphone is muted plays a warning sound and
  shows a red notice, like TeamSpeak. Turn it off under **Options ▸ Playback**.
* **Volume:** per-user volume, local mute and notification sounds.
* **Join / leave sounds:** a rising pair of notes when someone joins your channel, a
  falling pair when someone leaves (turn sounds off under **Options ▸ Playback**).
* **Auto away** (Voxa Server 1.16.0+): after 10 minutes without keyboard, mouse or
  talking you are set *Away (idle)*, and back online as soon as you return. The server
  admin chooses the time (up to 30 minutes, or off); turn it off for yourself under
  **Options ▸ Application**.

### Chat
* **Persistent encrypted history:** channel and server chat is stored encrypted on the
  server, so the history follows you to any PC (like Discord). Private messages are
  also supported.
* **Rich text** (Discord-style Markdown):
  * `**bold**`, `*italic*`, `__underline__`, `~~strikethrough~~` and `` `inline code` ``;
  * `> quotes`, `# headings`, and `- bullet` or `1. numbered` lists;
  * clickable links, including `[text](https://…)`;
  * ```` ```lang ```` **code blocks** with syntax highlighting and a Copy button.

  Shift+Enter starts a new line and Enter sends.
* **Emotes:** full-colour emoji.
  * Use the 😀 picker (search, categories, recently used).
  * Or type `:shortcode:`, e.g. `:fire:` `:+1:` `:joy:`. Suggestions appear as you type,
    and Tab inserts the first one.
  * Emoji-only messages are shown jumbo-sized.
* **Images** (Voxa Server 1.5.0+): send pictures with the 🖼 button, **Ctrl+V**
  (screenshots, copied pictures) or by dragging files in from Explorer.
  * Images are stored encrypted with the chat history.
  * Click one to view it full size and save it.
  * Up to 10 MB each; bigger photos are scaled down automatically.
* **YouTube links** get a preview card with the title, channel and thumbnail. Press ▶ to
  play the video inside Voxa, on the stage above the chat.
  * The player uses the Microsoft Edge WebView2 runtime that comes with Windows 10/11,
    via Microsoft's small redistributable `WebView2Loader.dll`.
  * Turn previews off under **Options ▸ Application ▸ Chat**.
* **Pinned messages** (Voxa Server 1.8.0+): pin messages with 📌 (hover or right-click).
  A **📌 Pinned** bar at the top of the chat lists them. Needs the **Pin messages**
  permission.
* **Direct messages** are kept **only on your PC**, never on the server, encrypted for
  your Windows account.
  * Delete single messages or a whole conversation (🗑 in its header). That removes your
    copy only; the other person keeps theirs.
  * Closing a conversation tab keeps its history; open it again from the People list.
* **👥 People** (top right, Voxa Server 1.13.0+): everyone who has been on the server.
  * Online people first, with their channel; then offline people with when they were
    last seen. 💬 marks people you have a saved conversation with.
  * Click anyone to message them.
* **Offline messages** (Voxa Server 1.13.0+): message someone who is offline and the
  server delivers it when they come online.
  * The server keeps it encrypted until then and deletes it after delivery.
  * Undelivered messages are dropped after 30 days, and at most 100 can wait per person.
  * When you come online, a window pops up showing who wrote, how many messages and
    the latest one, with **Open** to read them. The taskbar button flashes too.
* **Edit your messages** (Voxa Server 1.15.0+, *Edit own chat messages*): hover one of
  your channel or server chat messages ▸ ✏, right-click ▸ **Edit Message**, or press
  **Up** in an empty message box.
  * Enter saves, Esc cancels. Everyone sees the change at once, marked **(edited)**.
  * You can only edit your own messages, never images.
* **Moderation:**
  * Hover a message for 📋 Copy and 🗑 Delete, or right-click it. Deleted messages
    disappear for everyone and from the stored history.
  * You can always delete your own messages. Deleting other people's needs *Delete
    anyone's chat messages*.
  * *Clear entire chat history* adds a 🗑 button in the chat header that wipes a whole
    channel or server chat.

### Screen sharing
* **What you can share:** a whole screen or a single application, up to **60 fps**.
  Streams are relayed through the server, never peer-to-peer.
* **Capture:** screens use the Windows desktop duplication API, straight from the
  graphics card. If that is unavailable, for example during a UAC prompt or over Remote
  Desktop, Voxa falls back to GDI automatically.
* **The picker:** it has **Applications** and **Screens** tabs with live previews.
  Double-click one to go live.
* **Opens automatically:** when someone in your channel starts sharing, it opens above
  the chat.
  * With several streams there is a **tab per person**, plus **Side by side**.
  * Only the stream you look at is received, so hidden ones use no bandwidth and play
    no sound.
  * Close a stream with ✖ and it stays closed until it ends.
  * Turn automatic opening off in **Options ▸ Screen Share**.
* **Sound** (Voxa Server 1.6.0+, Windows 10 2004+):
  * A shared screen sends everything your PC plays except Voxa.
  * A shared application sends only its own sound.
  * Viewers get a mute button and a volume slider per stream.
* **Your own stream** shows on the stage as a preview of what viewers see.
* **Full screen:** 🖵 or double-click a stream to fill the whole monitor; Esc leaves.

### Channels
* **Drag & drop:** drag channels to reorder them.
  * Drop on the top or bottom edge of a channel to place it above or below.
  * Drop on the middle to make it a sub-channel.
  * Drag users onto a channel to move them.
* **Separators** (Voxa Server 1.12.0+): **Create Channel ▸ Kind: Separator**, like
  TeamSpeak's spacers.
  * A heading on the left, centre or right, or a line (`────`, `════`, `- - -`, …).
  * Nobody can join them.
  * They use TeamSpeak's tags (`[cspacer]Games`, `[*spacer]─`).
* **Clone Channel:** right-click a channel to copy it with its settings, password,
  channel groups and, if you like, its sub-channels.
* **Join power:** channels can require a minimum group *join power* (TeamSpeak style).
  Channels you can't enter show ⛔.

### Files
* **File browser** per channel:
  * Select: click, Ctrl+click, Shift+click, Select all, Ctrl+A.
  * Delete many items at once (Delete key).
  * **Upload Folder…** uploads whole folders with sub-folders. You can also drag files
    and folders in from Explorer.
  * Download several files, or whole folders, into a folder.
* **Transfers:**
  * They show progress, with **Cancel** and **Retry**.
  * Big batches are queued, running 3 transfers at a time.
  * Stalled transfers are detected, and the file connection reconnects by itself.
* **Limits:** admins set the max file size and per-channel storage under **Edit Virtual
  Server ▸ File transfers**.

### Administration
* Permissions (server and channel groups), privilege keys, bans, kick, move, poke, talk
  power and channel commander.
* **Bans** (Voxa Server 1.16.0+): ban by **unique ID**, **IP address** and / or
  **hardware**, any combination you tick.
  * A hardware ban stops new identities from the same PC (its network adapters' MAC
    addresses and the Windows machine ID; one match is enough).
  * Privacy: Voxa never sends the MAC addresses or machine ID. It sends each server
    scrambled tokens (hashes mixed with that server's verified fingerprint), which mean
    nothing to other servers and cannot be turned back into the addresses.
* **Personal permissions:** right-click a user ▸ **Permissions…** to grant individual
  permissions on top of their groups (needs *Manage groups*).
* **Traffic statistics** in the server info panel (admins only).
* **Web admin page** (Voxa Server 1.14.0+): manage the server from a browser with the
  same layout and themes (see the server's README).
  * **Sign in with Voxa** (Voxa Server 1.15.0+): the page asks Voxa, Voxa asks you
    which identity to use, and the browser is signed in. Your key never leaves Voxa;
    the proof only works on that page's web address, so a look-alike site gets nothing.
  * Or paste an identity exported from Voxa.

### Everyday conveniences
* Identities (export/import), bookmarks, and trust-on-first-use server fingerprint
  pinning.
* **Global hotkeys** under **Options ▸ Hotkeys**. They work in the background and from
  the tray: push-to-talk, mute microphone or speakers, push-to-mute, away, channel
  commander, start or stop screen sharing, bring Voxa to the front.
  * Use any key or mouse button (with Ctrl / Alt / Shift), or a **game controller**
    button: Xbox controllers (every button, the D-pad, LT / RT), PlayStation
    controllers, flight sticks, throttles, wheels and other game controllers (buttons
    and the hat switch).
  * Each action can have **several bindings**: **+ Add** another, ✖ removes one. Any of
    them triggers the action.
* **`voxa://host:port` links:** Voxa asks before connecting, because any web page can
  open such a link.
* **System tray:** closing the window (✕) keeps Voxa running in the notification area.
  Toggle it in **Options ▸ Application ▸ System tray**.
* **What's new:** after an update the change log is shown once. Open it any time under
  **About ▸ What's new**, with a list of every older version.
* **Updates:** Voxa checks this repository's GitHub releases at startup and every 12
  hours (**Options ▸ Application ▸ Updates**). Click the green ⬆ button, or **About ▸
  Check for updates**, to install. Voxa closes, upgrades and restarts by itself.

## Network
The client connects to the server on **TCP and UDP 9987** (or the port in the address).
No inbound ports are needed on the client side.

Lossless channels need about 0.8–1.5 Mbit/s per listener, sent by the server.

## Security
* **Connections:** Noise `XX_25519_ChaChaPoly_BLAKE2s` with mutual authentication.
  * Your identity key defines your unique ID.
  * The server's key is pinned on first connect, and a changed key is reported.
  * The second connection (files, screen share, images) must reach the same server.
* **Voice:** ChaCha20-Poly1305 with a per-session key and replay protection.
* **Your identity and saved passwords** are encrypted in `settings.json` with Windows
  data protection for your user account.
* **Direct messages** are stored in `%APPDATA%\Voxa\dm`, encrypted with ChaCha20-Poly1305
  under a random key that Windows data protection guards for your account. Each file is
  bound to its server, so it can't be read under another server or by another user.
* **Updates** are only installed from github.com, and only if they match the SHA-256
  checksum GitHub publishes. Windows' own `msiexec` installs them.
* **Downloads and chat content:**
  * Folder downloads never write outside the folder you pick.
  * Hostile chat messages, images and streams are size-limited, so they cannot freeze
    or exhaust Voxa.

## Building
Requirements: Rust (rustup) and PowerShell.

```powershell
.\build.ps1
```

The script installs the `stable-x86_64-pc-windows-gnullvm` toolchain if needed and
downloads portable LLVM-MinGW and WiX 3.14 into `%LOCALAPPDATA%\voxa-build-tools`. It
writes the installer to `dist\`. No C compiler or Visual Studio is required.

For development: `cargo run -p voxa-client`. Set `VOXA_CONFIG_DIR` to use a separate
settings folder (for example a second test user).

Tests:
* `cargo test -p voxa-client -p voxa-proto` runs the unit tests.
* End-to-end tests run against a running server, with
  `cargo run --release -p voxa-proto --example <name> -- 127.0.0.1:9987 <server data dir>`:
  * `smoke`, `image_test`, `pin_test` and `security_test`;
  * `feature_test` (codecs and the version requirement);
  * `spacer_test` (separators);
  * `stream_rate` (frame rate of a screen share).

## Releasing an update
1. Bump `version` in `Cargo.toml` (for example `1.2.0`) and commit.
2. Push a matching tag: `git tag v1.2.0` then `git push origin v1.2.0`.
3. GitHub Actions builds the MSI and publishes the release. Installed clients offer the
   update automatically.

To point a fork's updater at another repository, build with
`$env:VOXA_UPDATE_REPO = "you/your-fork"`.

## Protocol crate
`crates/voxa-proto` (wire protocol, crypto, codecs and updater) is shared with
[voxa-server](https://github.com/dylan0760/voxa-server). Keep both copies identical.
* **Incompatible changes:** bump `PROTOCOL_VERSION`.
* **New message types:** append them at the end, and only send them to peers whose
  version supports them.

## License
Dual-licensed under MIT or Apache-2.0, at your option. Third-party licenses are listed
in `THIRD-PARTY-NOTICES.txt` in the installed program folder.
