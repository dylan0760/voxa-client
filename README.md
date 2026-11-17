# Voxa (client)

Voxa is free, open source voice chat with persistent encrypted text chat, video calls,
screen sharing and channel calendars. It is written in Rust for **Windows and Android**
and has a TeamSpeak-6-style interface. There are no license keys, accounts,
registrations or fees.

This repository is the **client** (Windows and Android). The server lives at
[dylan0760/voxa-server](https://github.com/dylan0760/voxa-server).

Voxa is an independent clean-room implementation. It contains no TeamSpeak code or
assets and does not use the TeamSpeak protocol.

## Download
Get `Voxa-<version>-x64.msi` from the
[latest release](https://github.com/dylan0760/voxa-client/releases/latest) and run it.
The MSI is not code-signed. If Windows SmartScreen warns you, choose
**More info → Run anyway**.

**Android** (8.0 or newer): get `Voxa-<version>.apk` from the same release, open it on
the phone and allow installing it (Android asks once). See [Android](#android).

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
* **Themes:** pick one under **Options ▸ Application ▸ Themes**, where every theme is
  shown as a small preview of the window (the list scrolls; the window can be resized).
  * **Solid:** Dark, Light, 🌸 Sakura (cherry blossom), 🍼 Layette (baby blue),
    🍷 Crimson, 🌌 Midnight (purple), 🍵 Celadon (jade green), 🟡 Mustard (#FFDB58),
    Slate (soft charcoal) and Obsidian (near black).
  * **Two-tone:** one colour blends into another behind see-through panels, in the spirit
    of Discord's gradient themes. Light: Citrus, Candyfloss, Spearmint, Lo-fi and Dune.
    Dark: Raincloud, Afterglow, Blood Moon, Nightshade, Twilight, Synthwave,
    Pink Lemonade, Aurora, Woodland and 🌴 Vice City (1.31.0: a Miami sunset, orange into
    magenta with hot pink).
  * **Your picture** (1.32.0): any picture of yours behind see-through panels, like a
    wallpaper. **Options ▸ Application ▸ Themes ▸ Choose picture…** (PNG, JPEG, GIF, WebP
    or BMP, up to 40 MB). Voxa keeps a copy in its own folder, so it still shows if the
    original moves, and **Darken** keeps text easy to read. The picture stays on your
    device.
  * Older Voxa versions still read your settings: they use Dark or Light instead of a
    theme they don't know.
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
* **Fits any width** (1.31.0): on a narrow window or a big UI scale the side columns get
  narrower so the chat keeps room, the info panel steps aside when there is no space,
  and nothing overlaps: the voice card's buttons move to a row of their own, long
  channel and server names are shortened with "…" (hover for the whole name), and long
  values such as the server fingerprint wrap.
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
* **Devices** (**Options ▸ Capture / Playback**): by default Voxa uses Windows' **default
  communication devices**, like other voice apps; "Now:" shows which device that is. Pick
  **Default device (Windows)** or a specific device instead if you like. Voxa asks Windows
  not to turn other sounds down while it uses a communication device.
* **SteelSeries Sonar:** Sonar makes *Sonar - Chat* the communication device, so Voxa plays
  through Sonar's **Chat** channel and Sonar's Chat slider sets Voxa's volume separately
  from the game. Options ▸ Playback shows whether that is the case, with a **Use Sonar
  Chat and Sonar Microphone** button when it isn't.
* **Volume:** per-user volume, local mute and notification sounds.
* **Saves upload bandwidth:** when nobody else in your channel can hear you (you're alone,
  or the others are on the web page or have muted their speakers), Voxa sends no voice
  at all. Your speaking ring still lights up, and sending starts the moment someone who
  can listen joins.
* **Join / leave sounds:** a rising pair of notes when someone joins your channel, a
  falling pair when someone leaves (turn sounds off under **Options ▸ Playback**).
* **Auto away** (Voxa Server 1.16.0+): after 10 minutes without keyboard, mouse or
  talking you are set *Away (idle)*, and back online as soon as you return. The server
  admin chooses the time (up to 30 minutes, or off); turn it off for yourself under
  **Options ▸ Application**.

### On the server, in no channel (Voxa Server 1.17.0+)
* **Like Discord:** you can be connected to a server without being in a channel. You
  show up as online, use the server chat and direct messages, and double-click a channel
  to join it.
  * Turn off **Options ▸ Application ▸ Join a channel when connecting** to always connect
    this way (a bookmark's default channel is still joined).
  * **🚪 Leave the channel** and stay on the server: the 🚪 button on the "Voice
    Connected" card at the bottom left, your channel's right-click menu, its info panel,
    or the menu under your name.
  * Outside a channel nothing is transmitted, and there is no channel chat, files or
    screen sharing.
* **People:** everyone who is online is listed under **👥 People** with where they are
  ("in Lobby" or "on the server") and their status. The channel tree only shows people in
  channels, so a busy server doesn't push the channels out of view.

### Status
Click your name at the bottom left to set your status. Voxa remembers it for next time.

| Status | Others see | What it does |
|---|---|---|
| 🟢 **Online** | Online (the default) | |
| 🌙 **Away** | Away, with an optional message | Also set automatically when you are idle (auto away) |
| ⛔ **Do not disturb** | A ⛔ next to your name | No notification sounds, and pokes are only logged |
| 👻 **Invisible** | Offline, while you are not in a channel | You can still chat and get direct messages. Joining a channel makes you visible there |

* Invisible needs Voxa Server 1.17.0; Away works on every server.
* Server administrators (*Edit virtual server*) still see invisible people, so they can
  moderate.

### Chat
* **Persistent encrypted history:** channel chat is stored encrypted on the server, so
  the history follows you to any PC (like Discord). Direct messages and group chats are
  also supported.
* **The 🖧 Server tab** shows what happens on the server: who connects and leaves, where
  you are, the welcome message. Nobody can write there (Voxa Server 1.19.0 refuses it):
  chat in a channel, or with direct and group messages.
  * On Voxa Server 1.20.0+ the events are only for people with the **See server events**
    permission (Server Admins by default). Everyone else sees the server's **welcome
    message** there instead.
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
* **Messages window** (like Steam's friends and chat window): a window of its own that
  you can move to another screen.
  * **Left:** a search box, your **chats** (direct and group, with unread counts), **➕ New
    group chat**, then everyone on the server: **online** (with their status and channel)
    and **offline** (with when they were last seen). Click someone to chat; right-click
    for **Send message**, **Invite to group chat** and **New group chat with them**.
  * **Right:** the selected chat with its own message box, emoji, images (button, Ctrl+V
    or drag and drop) and 🗑.
  * Open it from the tray (**People on this server**), with **⧉ Open in its own window**
    in the 👥 People panel, **💬 Messages** above the chat, or by clicking or
    double-clicking someone.
  * **New messages, like Windows does it:** when the window is closed or minimized, it
    waits **minimized in the taskbar, flashing**, and a sound plays; it never jumps in
    front of your game. When the window is open on screen, the message just appears (no
    sound). Nothing pops up or plays while you're *Do not disturb*. Works while Voxa sits
    in the tray too.
  * Closing the window or a chat keeps everything; it all comes back next time.
  * Prefer tabs in the main window? Turn off **Options ▸ Application ▸ Open direct
    messages in their own window**.
* **Group chats** (Voxa Server 1.19.0+), like Steam's: chat with several people at once.
  * **➕ New group chat** in the Messages window: give it a name (or leave it empty for the
    names of the people), tick the people (online or offline) and start it.
  * In the chat's header: the people in it, **➕ Add people**, **✏ Rename**, **🚪 Leave**
    and 🗑 (deletes your copy of the history from this PC).
  * Any member can add people, rename it or leave; up to 50 people per group chat.
  * Like direct messages, the history is kept only on the members' PCs (encrypted). The
    server only keeps a message for members who are offline, until they connect.
  * Pictures can be sent too; only the chat's members can open them.
* **Direct messages** are kept **only on your PC**, never on the server, encrypted for
  your Windows account.
  * Delete single messages or a whole conversation (🗑 in its header). That removes your
    copy only; the other person keeps theirs.
  * Closing a conversation keeps its history; open it again from the People list.
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

### Video calls (Voxa Server 1.21.0+)
* **Your camera in your channel**, like Discord: 📷 in the voice card or above the chat
  turns it on or off. Everyone in the channel sees it; leaving the channel turns it off.
* **Group video like Teams:** as soon as a camera is on, everyone in the call gets a tile
  above the chat: cameras first, the others with their initials, a green frame around
  whoever talks. Click a tile to show it large, click again for everyone. Calls with
  more than 25 people show the first 25 and how many more are there. Drag the edge to
  make the video bigger or smaller.
* **Options ▸ Video:** which camera, picture size (360p, 480p or 720p), frame rate and
  image quality. Phones send upright (portrait) pictures.
* **Calls with a group chat or someone you message:** 📞 in the chat's header starts a
  call. Voxa makes a private, password-protected temporary channel for it and posts a
  **Join call** card in the chat; only the people in that chat get the password. The
  channel disappears when everyone has left. Calls need the permission to create
  temporary channels (normal members have it).
* **Call someone from the People list** (1.31.0): 📞 next to anyone online in 👥 People
  (or right-click them, also in the Messages window) opens your conversation with them
  and starts a call the same way. It is a voice call; turn your camera on if you like.
* **Encrypted:** voice and video are encrypted between every member and the server (see
  [Security](#security)). 🔒 in the corner of the video says so.
* Needs the *Share screen / applications and camera* permission in the channel.

### Channel calendar (Voxa Server 1.21.0+)
* **📅 Calendar** next to the channel's chat tab: what is arranged in this channel,
  e.g. a game on Monday at 8 pm for 8 players.
* **Several calendars per channel** (tabs), e.g. *Raids* and *Tournaments*.
* **Upcoming** lists what is coming, day by day; **Month** shows a month grid
  (double-click a day to arrange something on it).
* **An event** has a title, day and time (your local time), length, details and
  optionally a maximum number of people. Open it to answer **Going**, **Maybe** or
  **Can't** and to see who answered. When it is full, nobody else can say they are going.
* **Reminders:** 10 minutes before an event you are going to, Voxa reminds you with a
  sound. New events are announced in the channel chat.
* **Who may do what** is up to the server's permissions, per channel:
  * *See the channel calendar* (everyone by default);
  * *Answer calendar invitations* (normal members by default);
  * *Create and edit calendar events* and *Create, rename and delete calendars* (the
    channel's admins by default).
* Only the people in the channel see its calendar, so a password channel's calendar stays
  inside it.

### Server calendar (Voxa Server 1.23.0+, Voxa 1.31.0)
* **📆 Server calendar** next to the Server tab, and 📆 in the voice card at the bottom
  left (1.32.0): one calendar for the whole server, independent of the channel calendars,
  there whatever channel you are in, or when you are in none. It works like a channel's calendar: several
  calendars (tabs), Upcoming and Month, answers, limits and reminders.
* **Where to meet:** an event can name a channel, e.g. *Game night, Friday 8 pm in
  Racing*. Events show 📍 and the channel; **➡ Join** in the event takes you there.
* **Who may do what** is set by the same four calendar permissions, taken from your
  server groups (and personal permissions) rather than a channel: by default everyone
  sees it, normal members answer, and Server Admins arrange events. Give *Create and edit
  calendar events* to a server group to let its members arrange game nights.

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
  * Turn automatic opening off in **Options ▸ Video**.
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
* **Remove someone from the server** (Voxa Server 1.23.0+): right-click someone offline
  in 👥 People (or the Messages window) ▸ **Remove from server…**. The server forgets
  their groups, personal permissions, channel groups, picture and waiting messages; their
  chat messages stay, and they start fresh if they come back (ban them to keep them out).
  Needs *Create, edit and delete groups*, and only works on people without permissions
  you lack.
* **Permission search** (1.30.0): type in the 🔍 box above a permission list (groups and
  personal permissions) to show only the matching permissions, e.g. `calendar` or
  `chat delete`; the words can be in any order. The Groups dialog opens on the first group.
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
  Toggle it in **Options ▸ Application ▸ System tray**. Right-click the tray icon for:
  * **People on this server**: opens the Messages window with everyone on the server you
    are connected to (like Steam's friends list), or the 👥 People list with chat tabs;
  * **Messages**: opens the Messages window on your chats;
  * mute microphone / speakers, disconnect and quit.
* **Connect on startup:** tick **Connect when Voxa starts** on a bookmark (**Bookmarks**;
  one bookmark at a time). Voxa connects to it whenever it starts, also with Windows in
  the tray. If the network or the server isn't up yet (right after signing in), it tries
  again every 10 seconds for a minute; a refusal (wrong password, ban, update needed)
  is shown at once.
* **Start with Windows:** **Options ▸ Application ▸ Start Voxa when I sign in to
  Windows** starts Voxa hidden in the system tray when you sign in (a per-user entry
  under `HKCU\…\CurrentVersion\Run`; with the tray turned off, the window opens instead).
* **What's new:** after an update the change log is shown once. Open it any time under
  **About ▸ What's new**, with a list of every older version.
* **Updates:** Voxa checks this repository's GitHub releases at startup and every 12
  hours (**Options ▸ Application ▸ Updates**). Click the green ⬆ button, or **About ▸
  Check for updates**, to install. Voxa closes, upgrades and restarts by itself.

## Android
The same Voxa as on Windows, for phones and tablets (Android 8.0 or newer; arm64, and
x86_64 for Chromebooks).
* **Laid out like Discord's app** (1.31.0): the chat fills the screen.
  * **☰ at the top left** (or swiping right) slides in the servers (🏠 Home, your
    servers, ➕, bookmarks, identities, settings) next to the channel list, with the
    voice card and your own panel at the bottom.
  * **👥 at the top right** (or swiping left) slides in the info and 👥 People.
  * Tap the dimmed chat or swipe back to close a panel. Joining a channel or opening a
    conversation closes it by itself; tapping someone in the channel list shows who they
    are.
  * **Android's back button:** closes a menu, page or the right panel first; from the
    chat it opens the channels; from there Voxa goes to the background and stays
    connected.
  * Not on a server: the Home screen (bookmarks and recent servers) with the same ☰.
  * Tablets in landscape get the Windows layout.
* **Everything else works like on Windows:** voice, chat, direct and group messages,
  pictures, files, video calls, the calendar, watching screen shares, themes, bookmarks
  and identities. Dialogs open as full-screen pages with finger-sized buttons, ticks and
  lists; Android's back button closes them (and open menus) first.
* **Touch, like Discord** (1.30.0):
  * **tap a channel** to join it (the arrow still opens or closes it);
  * **press and hold** a message, person or channel for its menu (edit, copy text, pin,
    delete, poke, move…), where Windows uses a right-click;
  * **copy and paste:** while you type, a bar above the keyboard offers **Paste**,
    **Copy**, **Cut** and **Select all**, using Android's clipboard (so text moves between
    Voxa and other apps). **Copy text** in a message's menu copies it too. Voxa only reads
    the clipboard when you tap Paste;
  * closing the keyboard ends typing, so the bar never covers the page.
* **Push-to-talk** is a big **Hold to talk** button under the chat while you are in a
  channel (voice activation is the default).
* **Pictures, uploads and your avatar** are chosen in Voxa's own file browser. The first
  time, Voxa asks for **All files access** (Android 11+) or the storage permission.
  Downloads and saved pictures go to `Download/Voxa`.
* **Permissions:** the microphone when Voxa starts, the camera when you first turn it on,
  and notifications on Android 13+.
* **voxa:// links** from web pages open Voxa.
* **Updates:** Voxa finds new versions like on Windows (**About ▸ Check for updates**).
  **Download and install** fetches the newest APK from GitHub, checks it against GitHub's
  checksum and hands it to Android's installer, which updates Voxa in place (settings
  stay). The first time, Android asks you to allow Voxa to install apps: turn it on and go
  back. (Voxa 1.31.0 and older could not install updates themselves and opened the
  download in the browser; from 1.32.0 it works in the app.)
* **Not on phones:** sharing your screen (you can watch others'), global hotkeys, the
  tray and starting with Windows. Android may stop the microphone while Voxa is in the
  background, so keep it open while you talk.
* **Your data** (identity, settings, direct messages) is in Voxa's private app storage,
  which other apps can't read. Android backups are turned off for Voxa so your identity
  key never leaves the phone that way.

## Network
The client connects to the server on **TCP and UDP 9987** (or the port in the address).
No inbound ports are needed on the client side.

Lossless channels need about 0.8–1.5 Mbit/s per listener, sent by the server.

## Security
* **Connections:** Noise `XX_25519_ChaChaPoly_BLAKE2s` with mutual authentication.
  * Your identity key defines your unique ID.
  * The server's key is pinned on first connect, and a changed key is reported.
  * The second connection (files, screen share, images) must reach the same server.
* **Voice:** every packet is encrypted and authenticated with ChaCha20-Poly1305, under a
  fresh random 256-bit key per connection that comes over the encrypted connection,
  with replay protection. This is always on, whatever a channel's setting says.
* **Video and screen sharing** go over the second (Noise-encrypted) connection, so they
  are encrypted too. The server relays them to the people in your channel only.
* **Calls** started from a chat use a private channel with a random 20-character
  password that only that chat's members get.
* **Your identity and saved passwords** are encrypted in `settings.json` with Windows
  data protection for your user account (on Android: kept in Voxa's private app storage).
  If `settings.json` is ever damaged, Voxa keeps a copy (`settings.damaged-<time>.json`,
  next to it) before starting with defaults, so your identities can be recovered.
* **Direct messages** are stored in `%APPDATA%\Voxa\dm`, encrypted with ChaCha20-Poly1305
  under a random key that Windows data protection guards for your account. Each file is
  bound to its server, so it can't be read under another server or by another user.
* **Updates** are only installed from github.com, and only if they match the SHA-256
  checksum GitHub publishes. Windows' own `msiexec` installs them.
* **Invisible means invisible** (Voxa Server 1.17.0+): the server leaves you out of
  everyone else's client list, chat history, avatars and group member lists, and keeps
  your "last seen" time from when you were last visible. Anyone who can't see you gets
  "Client not found" when they try to poke or message your session. A direct message to
  your identity is answered "kept for later" but reaches you right away.
* **Downloads and chat content:**
  * Folder downloads never write outside the folder you pick.
  * Hostile chat messages, images and streams are size-limited, so they cannot freeze
    or exhaust Voxa.

## Building
Requirements: Rust (rustup) and PowerShell.

```powershell
.\build.ps1        # Windows: dist\Voxa-<version>-x64.msi
.\build-apk.ps1    # Android: dist\Voxa-<version>.apk
```

The Android build needs the Rust targets `aarch64-linux-android` and
`x86_64-linux-android`, the Android NDK (`ANDROID_NDK_HOME`), the SDK with build-tools
and a platform (`ANDROID_HOME`) and Java (for `apksigner`). Every version must be
signed with the same key (`VOXA_APK_KEYSTORE` / `VOXA_APK_KEYPASS`), or Android refuses
to update the app. The Android app is `crates/voxa-android`: it builds the client's own
code as the library an Android NativeActivity loads.

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
  * `group_test`, `video_test` and `calendar_test` (group chats, video, calendar);
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
