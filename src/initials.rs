//! The letter an account's name files under in the phone list's index: A to Z, or `#`. Latin letters are their own
//! (accented ones as their base letter, full-width ones as the plain), a Chinese character files under its pinyin's
//! first letter, and anything else (digits, signs, other scripts) under `#`, which the index lists last.
use pinyin::ToPinyin;

/// Where everything without a letter files.
pub const OTHER: char = '#';

/// The letter `name` files under.
pub fn initial(name: &str) -> char {
    let Some(c) = name.trim_start().chars().next() else { return OTHER };
    if let Some(letter) = latin(c) {
        return letter;
    }
    c.to_pinyin()
        .and_then(|p| p.first_letter().chars().next())
        .map(|l| l.to_ascii_uppercase())
        .filter(char::is_ascii_uppercase)
        .unwrap_or(OTHER)
}

/// The index's order: A to Z, then `#`.
pub fn order(letter: char) -> u32 {
    if letter == OTHER { u32::MAX } else { letter as u32 }
}

fn latin(c: char) -> Option<char> {
    if c.is_ascii_alphabetic() {
        return Some(c.to_ascii_uppercase());
    }
    // Full-width forms (U+FF21 A to U+FF3A Z, U+FF41 a to U+FF5A z).
    let code = c as u32;
    if (0xFF21..=0xFF3A).contains(&code) {
        return char::from_u32(code - 0xFF21 + 'A' as u32);
    }
    if (0xFF41..=0xFF5A).contains(&code) {
        return char::from_u32(code - 0xFF41 + 'A' as u32);
    }
    // The accented letters of Latin-1 and Latin Extended-A, by their base letter.
    let base = match c.to_lowercase().next()? {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'A',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'C',
        'ď' | 'đ' => 'D',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'E',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'G',
        'ĥ' | 'ħ' => 'H',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'I',
        'ĵ' => 'J',
        'ķ' => 'K',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'L',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'N',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'O',
        'ŕ' | 'ŗ' | 'ř' => 'R',
        'ś' | 'ŝ' | 'ş' | 'š' | 'ß' => 'S',
        'ţ' | 'ť' | 'ŧ' => 'T',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'U',
        'ŵ' => 'W',
        'ý' | 'ÿ' | 'ŷ' => 'Y',
        'ź' | 'ż' | 'ž' => 'Z',
        _ => return None,
    };
    Some(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_names_file_under_their_letter() {
        assert_eq!(initial("github"), 'G');
        assert_eq!(initial("  AWS"), 'A');
        assert_eq!(initial("Ölkassen"), 'O');
        assert_eq!(initial("élan"), 'E');
        assert_eq!(initial("Ｇｏｏｇｌｅ"), 'G');
    }

    #[test]
    fn chinese_names_file_under_their_pinyin() {
        assert_eq!(initial("微信"), 'W');
        assert_eq!(initial("支付宝"), 'Z');
        assert_eq!(initial("阿里云"), 'A');
        assert_eq!(initial("腾讯 QQ"), 'T');
    }

    #[test]
    fn everything_else_files_last() {
        assert_eq!(initial("1Password"), OTHER);
        assert_eq!(initial("@work"), OTHER);
        assert_eq!(initial("カカオ"), OTHER);
        assert_eq!(initial(""), OTHER);
        assert!(order('Z') < order(OTHER));
        assert!(order('A') < order('B'));
    }
}
