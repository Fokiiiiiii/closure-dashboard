#[must_use]
pub fn get(lang: &str, key: &str) -> &'static str {
    let en = lang == "en";
    match (key, en) {
        ("title", false) => "ダッシュボード",
        ("title", true) => "Dashboard",
        ("login", false) => "Discord でログイン",
        ("login", true) => "Sign in with Discord",
        ("dev", false) => "開発用ログイン",
        ("dev", true) => "Development login",
        ("logout", false) => "ログアウト",
        ("logout", true) => "Sign out",
        ("servers", false) => "サーバーを選択",
        ("servers", true) => "Choose a server",
        ("empty", false) => "利用できるサーバーはありません",
        ("empty", true) => "No servers available",
        ("overview", false) => "概要",
        ("overview", true) => "Overview",
        ("modules", false) => "モジュール",
        ("modules", true) => "Modules",
        ("bot", _) => "Bot",
        ("connected", false) => "接続中",
        ("connected", true) => "Connected",
        ("active", false) => "有効なモジュール",
        ("active", true) => "Active modules",
        ("last", false) => "最後の変更",
        ("last", true) => "Last change",
        ("recent", false) => "最近の変更",
        ("recent", true) => "Recent changes",
        ("none", false) => "変更はありません",
        ("none", true) => "No changes yet",
        ("enabled", false) => "有効",
        ("enabled", true) => "Enabled",
        ("disabled", false) => "無効",
        ("disabled", true) => "Disabled",
        ("denied", false) => "このサーバーへのアクセス権がありません",
        ("denied", true) => "You cannot access this server",
        ("unavailable", false) => "Bot に接続できません",
        ("unavailable", true) => "Cannot connect to the bot",
        ("notfound", false) => "このサーバーには参加していません",
        ("notfound", true) => "The bot is not in this server",
        ("invalid", false) => "設定を保存できませんでした",
        ("invalid", true) => "Could not save the configuration",
        ("conflict", false) => "設定が同時に変更されました。もう一度お試しください",
        ("conflict", true) => "The configuration changed concurrently. Try again",
        ("oauth", false) => "Discord 認証に失敗しました",
        ("oauth", true) => "Discord authentication failed",
        ("theme", false) => "テーマ",
        ("theme", true) => "Theme",
        ("auto", false) => "自動",
        ("auto", true) => "Auto",
        ("light", false) => "ライト",
        ("light", true) => "Light",
        ("dark", false) => "ダーク",
        ("dark", true) => "Dark",
        ("apply", false) => "適用",
        ("apply", true) => "Apply",
        _ => "Error",
    }
}

#[must_use]
pub fn relative_time(lang: &str, timestamp: i64, now: u64) -> String {
    let elapsed = now.saturating_sub(u64::try_from(timestamp).unwrap_or(0));
    if lang == "en" {
        if elapsed < 60 {
            return "just now".into();
        }
        let (n, unit) = if elapsed < 3600 {
            (elapsed / 60, "minute")
        } else if elapsed < 86400 {
            (elapsed / 3600, "hour")
        } else {
            (elapsed / 86400, "day")
        };
        if n == 1 && unit == "day" {
            return "yesterday".into();
        }
        format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
    } else {
        if elapsed < 60 {
            return "たった今".into();
        }
        if elapsed < 3600 {
            return format!("{} 分前", elapsed / 60);
        }
        if elapsed < 86400 {
            return format!("{} 時間前", elapsed / 3600);
        }
        if elapsed < 172_800 {
            return "昨日".into();
        }
        format!("{} 日前", elapsed / 86400)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn relative_time_uses_supplied_clock() {
        for (elapsed, ja, en) in [
            (0, "たった今", "just now"),
            (60, "1 分前", "1 minute ago"),
            (7200, "2 時間前", "2 hours ago"),
            (86400, "昨日", "yesterday"),
            (259_200, "3 日前", "3 days ago"),
        ] {
            assert_eq!(
                super::relative_time("ja", 1_000_000 - elapsed, 1_000_000),
                ja
            );
            assert_eq!(
                super::relative_time("en", 1_000_000 - elapsed, 1_000_000),
                en
            );
        }
        assert_eq!(super::relative_time("en", 2_000_000, 1_000_000), "just now");
    }
}
