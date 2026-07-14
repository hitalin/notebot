//! メンションテキストのコマンド解析。
//!
//! 文頭のメンション連続 (`@bot` / `@bot@host` / リプライチェーンで付く
//! 他ユーザー宛含む) をすべて読み飛ばし、最初の通常トークンをコマンド名
//! (小文字)、残りを空白区切りの引数とする。MFM の完全パースはしない。

/// `Some((コマンド名, 引数))` を返す。本文がメンションのみ・空なら `None`。
pub(crate) fn parse_command(text: &str) -> Option<(String, Vec<String>)> {
    let mut rest = text.trim_start();
    while let Some(after) = rest.strip_prefix('@') {
        let end = after
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after.len());
        rest = after[end..].trim_start();
    }
    let mut tokens = rest.split_whitespace();
    let cmd = tokens.next()?.to_lowercase();
    let args: Vec<String> = tokens.map(String::from).collect();
    Some((cmd, args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_command() {
        assert_eq!(
            parse_command("@bot ping"),
            Some(("ping".to_string(), vec![]))
        );
    }

    #[test]
    fn parses_full_acct_mention_and_args() {
        assert_eq!(
            parse_command("@bot@misskey.example dice 6 2"),
            Some(("dice".to_string(), vec!["6".to_string(), "2".to_string()]))
        );
    }

    #[test]
    fn skips_reply_chain_mentions() {
        // リプライチェーンでは他ユーザー宛メンションが先頭に付く
        assert_eq!(
            parse_command("@alice @bot ping"),
            Some(("ping".to_string(), vec![]))
        );
    }

    #[test]
    fn command_lookup_is_case_insensitive() {
        assert_eq!(
            parse_command("@bot PING"),
            Some(("ping".to_string(), vec![]))
        );
    }

    #[test]
    fn mention_only_is_not_a_command() {
        assert_eq!(parse_command("@bot"), None);
        assert_eq!(parse_command("@bot @alice"), None);
        assert_eq!(parse_command("  "), None);
    }
}
