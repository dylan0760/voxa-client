//! Emoji ("emotes") for chat: picker categories, `:shortcode:` names and helpers.

pub struct Emoji {
    pub ch: &'static str,
    pub code: &'static str,
}

pub struct Category {
    pub icon: &'static str,
    pub name: &'static str,
    pub items: &'static [Emoji],
}

macro_rules! e {
    ($($ch:literal $code:literal),* $(,)?) => { &[$(Emoji { ch: $ch, code: $code }),*] };
}

pub static CATEGORIES: &[Category] = &[
    Category {
        icon: "😀",
        name: "Smileys",
        items: e![
            "😀" "grinning", "😃" "smiley", "😄" "smile", "😁" "grin", "😆" "laughing", "😅" "sweat_smile",
            "🤣" "rofl", "😂" "joy", "🙂" "slight_smile", "🙃" "upside_down", "😉" "wink", "😊" "blush",
            "😇" "innocent", "🥰" "smiling_hearts", "😍" "heart_eyes", "🤩" "star_struck", "😘" "kiss",
            "😋" "yum", "😛" "tongue", "😜" "wink_tongue", "🤪" "zany", "🤑" "money_mouth", "🤗" "hug",
            "🤭" "hand_over_mouth", "🤫" "shush", "🤔" "thinking", "🤐" "zipper_mouth", "🤨" "raised_eyebrow",
            "😐" "neutral", "😑" "expressionless", "😶" "no_mouth", "😏" "smirk", "😒" "unamused",
            "🙄" "eye_roll", "😬" "grimace", "😌" "relieved", "😔" "pensive", "😪" "sleepy", "🤤" "drool",
            "😴" "sleeping", "😷" "mask", "🤒" "sick", "🤢" "nauseated", "🤮" "vomit", "🥵" "hot",
            "🥶" "cold", "🥴" "woozy", "😵" "dizzy", "🤯" "mind_blown", "🤠" "cowboy", "🥳" "party",
            "😎" "sunglasses", "🤓" "nerd", "🧐" "monocle", "😕" "confused", "😟" "worried",
            "🙁" "slight_frown", "😮" "open_mouth", "😲" "astonished", "😳" "flushed", "🥺" "pleading",
            "😦" "frowning", "😨" "fearful", "😰" "anxious", "😥" "sad_relieved", "😢" "cry", "😭" "sob",
            "😱" "scream", "😖" "confounded", "😣" "persevere", "😞" "disappointed", "😓" "sweat",
            "😩" "weary", "😫" "tired", "🥱" "yawn", "😤" "triumph", "😡" "rage", "😠" "angry",
            "🤬" "cursing", "😈" "smiling_imp", "👿" "imp", "💀" "skull", "💩" "poop", "🤡" "clown",
            "👻" "ghost", "👽" "alien", "🤖" "robot", "😺" "smiley_cat", "😹" "joy_cat", "😻" "heart_eyes_cat",
        ],
    },
    Category {
        icon: "👍",
        name: "People",
        items: e![
            "👋" "wave", "🤚" "raised_back_of_hand", "✋" "hand", "🖖" "vulcan", "👌" "ok_hand", "🤌" "pinched",
            "🤏" "pinch", "✌️" "v", "🤞" "fingers_crossed", "🤟" "love_you", "🤘" "metal", "🤙" "call_me",
            "👈" "point_left", "👉" "point_right", "👆" "point_up", "👇" "point_down", "👍" "+1",
            "👎" "-1", "✊" "fist", "👊" "punch", "👏" "clap", "🙌" "raised_hands", "👐" "open_hands",
            "🤲" "palms_up", "🤝" "handshake", "🙏" "pray", "💪" "muscle", "🫡" "salute", "👀" "eyes",
            "🧠" "brain", "👂" "ear", "👄" "lips", "🙋" "raising_hand", "🤦" "facepalm", "🤷" "shrug",
            "🙇" "bow", "💃" "dancer", "🕺" "man_dancing", "🧑‍💻" "technologist", "🥷" "ninja", "🧙" "mage",
        ],
    },
    Category {
        icon: "❤️",
        name: "Hearts",
        items: e![
            "❤️" "heart", "🧡" "orange_heart", "💛" "yellow_heart", "💚" "green_heart", "💙" "blue_heart",
            "💜" "purple_heart", "🖤" "black_heart", "🤍" "white_heart", "🤎" "brown_heart", "💔" "broken_heart",
            "💕" "two_hearts", "💖" "sparkling_heart", "💯" "100", "💢" "anger", "💥" "boom", "💫" "dizzy_star",
            "💦" "sweat_drops", "💤" "zzz", "💬" "speech", "💭" "thought", "🔥" "fire", "✨" "sparkles",
            "⭐" "star", "🌟" "star2", "⚡" "zap", "🎉" "tada", "🎊" "confetti", "🎈" "balloon", "🎁" "gift",
        ],
    },
    Category {
        icon: "🐶",
        name: "Animals & Nature",
        items: e![
            "🐶" "dog", "🐱" "cat", "🐭" "mouse", "🐹" "hamster", "🐰" "rabbit", "🦊" "fox", "🐻" "bear",
            "🐼" "panda", "🐨" "koala", "🐯" "tiger", "🦁" "lion", "🐮" "cow", "🐷" "pig", "🐸" "frog",
            "🐵" "monkey", "🙈" "see_no_evil", "🙉" "hear_no_evil", "🙊" "speak_no_evil", "🐔" "chicken",
            "🐧" "penguin", "🐦" "bird", "🦆" "duck", "🦅" "eagle", "🦉" "owl", "🐺" "wolf", "🐴" "horse",
            "🦄" "unicorn", "🐝" "bee", "🐛" "bug", "🦋" "butterfly", "🐢" "turtle", "🐍" "snake",
            "🐙" "octopus", "🦀" "crab", "🐬" "dolphin", "🐳" "whale", "🦈" "shark", "🐉" "dragon",
            "🌵" "cactus", "🌲" "tree", "🍀" "four_leaf_clover", "🍁" "maple_leaf", "🌸" "cherry_blossom",
            "🌹" "rose", "🌻" "sunflower", "🌙" "moon", "☀️" "sun", "🌈" "rainbow", "☁️" "cloud",
            "❄️" "snowflake", "🌊" "ocean",
        ],
    },
    Category {
        icon: "🍕",
        name: "Food & Drink",
        items: e![
            "🍎" "apple", "🍌" "banana", "🍉" "watermelon", "🍇" "grapes", "🍓" "strawberry", "🍒" "cherries",
            "🍑" "peach", "🥑" "avocado", "🌶️" "hot_pepper", "🌽" "corn", "🥕" "carrot", "🥐" "croissant",
            "🍞" "bread", "🧀" "cheese", "🥓" "bacon", "🍔" "burger", "🍟" "fries", "🍕" "pizza",
            "🌭" "hotdog", "🌮" "taco", "🌯" "burrito", "🍜" "ramen", "🍣" "sushi", "🍿" "popcorn",
            "🍩" "doughnut", "🍪" "cookie", "🎂" "birthday", "🍰" "cake", "🍫" "chocolate", "🍬" "candy",
            "☕" "coffee", "🍵" "tea", "🥤" "cup", "🍺" "beer", "🍻" "beers", "🍷" "wine", "🍸" "cocktail",
            "🥂" "cheers", "🧃" "juice",
        ],
    },
    Category {
        icon: "🎮",
        name: "Activities",
        items: e![
            "🎮" "video_game", "🕹️" "joystick", "🎲" "game_die", "♟️" "chess", "🧩" "puzzle", "🎯" "dart",
            "🎳" "bowling", "⚽" "soccer", "🏀" "basketball", "🏈" "football", "⚾" "baseball", "🎾" "tennis",
            "🏐" "volleyball", "🏓" "ping_pong", "🥊" "boxing", "🏆" "trophy", "🥇" "first_place",
            "🥈" "second_place", "🥉" "third_place", "🏅" "medal", "🎵" "musical_note", "🎶" "notes",
            "🎤" "microphone", "🎧" "headphones", "🎸" "guitar", "🎹" "piano", "🥁" "drum", "🎬" "clapper",
            "🎨" "art", "🎭" "theater", "🎪" "circus", "🚀" "rocket", "🛸" "ufo", "🗡️" "dagger",
            "⚔️" "crossed_swords", "🛡️" "shield", "🏹" "bow_and_arrow", "💣" "bomb",
        ],
    },
    Category {
        icon: "💡",
        name: "Objects",
        items: e![
            "💻" "computer", "🖥️" "desktop", "⌨️" "keyboard", "🖱️" "mouse_computer", "📱" "phone", "📷" "camera",
            "📺" "tv", "🔊" "loud_sound", "🔇" "mute", "🔔" "bell", "📢" "loudspeaker", "💡" "bulb",
            "🔦" "flashlight", "📚" "books", "📝" "memo", "📌" "pushpin", "📎" "paperclip", "✂️" "scissors",
            "🔒" "lock", "🔑" "key", "🔨" "hammer", "🛠️" "tools", "⚙️" "gear", "🧲" "magnet", "💰" "moneybag",
            "💎" "gem", "⏰" "alarm_clock", "⌛" "hourglass", "📅" "calendar", "📦" "package", "🎟️" "ticket",
            "🚗" "car", "✈️" "airplane", "🏠" "house", "🌍" "earth",
        ],
    },
    Category {
        icon: "✅",
        name: "Symbols",
        items: e![
            "✅" "white_check_mark", "✔️" "check", "❌" "x", "❎" "negative_squared_cross", "⭕" "o",
            "❗" "exclamation", "❓" "question", "‼️" "bangbang", "⁉️" "interrobang", "⚠️" "warning",
            "🚫" "no_entry", "⛔" "stop", "🔴" "red_circle", "🟠" "orange_circle", "🟡" "yellow_circle",
            "🟢" "green_circle", "🔵" "blue_circle", "🟣" "purple_circle", "⚫" "black_circle", "⚪" "white_circle",
            "➕" "plus", "➖" "minus", "➗" "divide", "✖️" "multiply", "♾️" "infinity", "💲" "dollar",
            "🔝" "top", "🆗" "ok", "🆕" "new", "🆒" "cool", "🆘" "sos", "🔁" "repeat", "▶️" "play",
            "⏸️" "pause", "⏹️" "stop_button", "⏺️" "record", "🔀" "shuffle", "➡️" "arrow_right",
            "⬅️" "arrow_left", "⬆️" "arrow_up", "⬇️" "arrow_down", "🏳️" "white_flag", "🏁" "checkered_flag",
        ],
    },
];

pub fn all() -> impl Iterator<Item = &'static Emoji> {
    CATEGORIES.iter().flat_map(|c| c.items.iter())
}

/// Common aliases people type out of habit (Discord / Slack / GitHub style).
static ALIASES: &[(&str, &str)] = &[
    ("thumbsup", "👍"),
    ("thumbsdown", "👎"),
    ("like", "👍"),
    ("lol", "😂"),
    ("laugh", "😆"),
    ("happy", "😊"),
    ("sad", "😢"),
    ("love", "❤️"),
    ("<3", "❤️"),
    ("ok", "👌"),
    ("gg", "🏆"),
    ("hype", "🔥"),
    ("check_mark", "✅"),
    ("thinking_face", "🤔"),
    ("shrug", "🤷"),
];

pub fn lookup(code: &str) -> Option<&'static str> {
    let code = code.to_ascii_lowercase();
    all().find(|e| e.code == code).map(|e| e.ch).or_else(|| ALIASES.iter().find(|(a, _)| *a == code).map(|(_, e)| *e))
}

/// Replaces complete `:shortcode:` occurrences with emoji.
pub fn replace_shortcodes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find(':') {
            Some(end) if end > 0 && end <= 32 && !after[..end].contains(char::is_whitespace) => match lookup(&after[..end]) {
                Some(e) => {
                    out.push_str(e);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push(':');
                    rest = after;
                }
            },
            _ => {
                out.push(':');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Autocomplete: if the text ends with `:partial` (2+ chars), returns matching emoji.
pub fn suggestions(text: &str, max: usize) -> (Option<usize>, Vec<&'static Emoji>) {
    // Skip the whole whitespace character: some (no-break space, ideographic space) are
    // more than one byte, and slicing inside them would panic.
    let token_start = text
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let token = &text[token_start..];
    let Some(q) = token.strip_prefix(':') else { return (None, Vec::new()) };
    if q.chars().count() < 2 || q.contains(':') {
        return (None, Vec::new());
    }
    let q = q.to_ascii_lowercase();
    let mut v: Vec<&Emoji> = all().filter(|e| e.code.starts_with(&q)).collect();
    v.extend(all().filter(|e| !e.code.starts_with(&q) && e.code.contains(&q)));
    v.truncate(max);
    (Some(token_start), v)
}

pub fn search(q: &str) -> Vec<&'static Emoji> {
    let q = q.trim().trim_matches(':').to_ascii_lowercase();
    all().filter(|e| e.code.contains(&q)).collect()
}

/// True when a message consists only of (at most 6) emoji, shown "jumbo" like Discord.
pub fn is_only_emoji(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > 30 {
        return false;
    }
    let mut count = 0;
    for ch in t.chars() {
        let c = ch as u32;
        let emojiish = (0x1F000..=0x1FAFF).contains(&c)
            || (0x2600..=0x27BF).contains(&c)
            || (0x2B00..=0x2BFF).contains(&c)
            || (0x2190..=0x21FF).contains(&c)
            || matches!(c, 0x200D | 0xFE0F | 0x20E3 | 0x203C | 0x2049 | 0x2122 | 0x2139 | 0x00A9 | 0x00AE | 0x3030 | 0x303D)
            || ch.is_whitespace();
        if !emojiish {
            return false;
        }
        if (0x1F000..=0x1FAFF).contains(&c) || (0x2600..=0x27BF).contains(&c) || (0x2B00..=0x2BFF).contains(&c) {
            count += 1;
        }
    }
    (1..=6).contains(&count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Multi-byte whitespace before a shortcode used to crash the client.
    #[test]
    fn suggestions_after_wide_whitespace() {
        for ws in ['\u{a0}', '\u{3000}', '\u{2003}', ' ', '\n'] {
            let text = format!("hi{ws}:fi");
            let (start, found) = suggestions(&text, 5);
            assert_eq!(start, Some(text.len() - 3), "{ws:?}");
            assert!(!found.is_empty());
        }
        assert_eq!(suggestions("\u{3000}", 5).0, None);
    }

    #[test]
    fn shortcodes() {
        assert_eq!(replace_shortcodes("gg :fire: :+1:"), "gg 🔥 👍");
        assert_eq!(replace_shortcodes("time 12:30:45"), "time 12:30:45");
        assert_eq!(replace_shortcodes(":nope: :smile:"), ":nope: 😄");
        assert_eq!(replace_shortcodes("a:b"), "a:b");
    }

    #[test]
    fn suggest() {
        let (start, v) = suggestions("hello :fi", 5);
        assert_eq!(start, Some(6));
        assert!(v.iter().any(|e| e.code == "fire"));
        assert!(suggestions("hello :f", 5).1.is_empty());
        assert!(suggestions("hello", 5).1.is_empty());
    }

    #[test]
    fn jumbo() {
        assert!(is_only_emoji("🔥🔥"));
        assert!(is_only_emoji("👍"));
        assert!(!is_only_emoji("gg 🔥"));
    }

    #[test]
    fn unique_codes() {
        let mut codes: Vec<&str> = all().map(|e| e.code).collect();
        codes.sort();
        let n = codes.len();
        codes.dedup();
        assert_eq!(n, codes.len(), "duplicate shortcode");
    }
}
