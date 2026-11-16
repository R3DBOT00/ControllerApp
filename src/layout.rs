//! Layout data taken from the behavior specification, not a Windows API port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Backspace,
    Enter,
    Escape,
    Tab,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Super,
    Control,
    Alt,
    AltGr,
    PrintScreen,
    Left,
    Up,
    Down,
    Right,
    Space,
    F(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Character(char, char),
    Key(Key),
    Layer,
}

#[derive(Clone, Debug)]
pub struct Keycap {
    pub label: String,
    pub shifted: String,
    pub units: f64,
    pub action: Action,
}
impl Keycap {
    fn character(a: char, b: char) -> Self {
        Self {
            label: a.to_string(),
            shifted: b.to_string(),
            units: 1.,
            action: Action::Character(a, b),
        }
    }
    fn special(label: &str, units: f64, action: Action) -> Self {
        Self {
            label: label.into(),
            shifted: label.into(),
            units,
            action,
        }
    }
    pub fn text(&self, shift: bool) -> &str {
        if shift { &self.shifted } else { &self.label }
    }
}
pub type Layout = Vec<Vec<Keycap>>;
fn letters(normal: &str, shifted: &str) -> Vec<Keycap> {
    normal
        .chars()
        .zip(shifted.chars())
        .map(|(a, b)| Keycap::character(a, b))
        .collect()
}
pub fn main_layout() -> Layout {
    let mut a = letters("1234567890-=", "!@#$%¨&*()_+");
    a.push(Keycap::special(
        "Backspace",
        2.2,
        Action::Key(Key::Backspace),
    ));
    let b = letters("qwertyuiop´[]", "QWERTYUIOP`{}");
    let mut c = letters("asdfghjklç~", "ASDFGHJKLÇ^");
    c.push(Keycap::special("Enter", 2.2, Action::Key(Key::Enter)));
    let mut d = letters("zxcvbnm,.", "ZXCVBNM<>");
    d.push(Keycap::special("Space", 3.2, Action::Key(Key::Space)));
    d.push(Keycap::special("FN", 1.4, Action::Layer));
    vec![a, b, c, d]
}
pub fn function_layout() -> Layout {
    use Key::*;
    vec![
        (1..=12)
            .map(|n| Keycap::special(&format!("F{n}"), 1., Action::Key(F(n))))
            .collect(),
        [
            ("Esc", Escape),
            ("Tab", Tab),
            ("Delete", Delete),
            ("Home", Home),
            ("End", End),
            ("Pg Up", PageUp),
            ("Pg Dn", PageDown),
        ]
        .into_iter()
        .map(|(s, k)| Keycap::special(s, 1., Action::Key(k)))
        .collect(),
        [
            ("Win", Super),
            ("Ctrl", Control),
            ("Alt", Alt),
            ("AltGr", AltGr),
            ("Prt Sc", PrintScreen),
        ]
        .into_iter()
        .map(|(s, k)| Keycap::special(s, 1.5, Action::Key(k)))
        .collect(),
        vec![
            Keycap::special("◄", 1., Action::Key(Left)),
            Keycap::special("▲", 1., Action::Key(Up)),
            Keycap::special("▼", 1., Action::Key(Down)),
            Keycap::special("►", 1., Action::Key(Right)),
            Keycap::special("ABC", 4., Action::Layer),
        ],
    ]
}
pub fn compose(accent: char, letter: char) -> Option<char> {
    let (base, composed) = match accent {
        '´' => ("aeiouc", "áéíóúç"),
        '~' => ("aon", "ãõñ"),
        '^' => ("aeiou", "âêîôû"),
        '`' => ("aeiou", "àèìòù"),
        _ => return None,
    };
    let lower = letter.to_lowercase().next()?;
    let n = base.chars().position(|c| c == lower)?;
    let result = composed.chars().nth(n)?;
    Some(if letter.is_uppercase() {
        result.to_uppercase().next()?
    } else {
        result
    })
}
pub fn decompose(c: char) -> Option<(char, char)> {
    for accent in ['´', '~', '^', '`'] {
        for base in "aeiouncAEIOUNC".chars() {
            if compose(accent, base) == Some(c) {
                return Some((accent, base));
            }
        }
    }
    None
}
