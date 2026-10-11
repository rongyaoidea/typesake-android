//! 个人词库：名单（可粘贴通讯录）导入 + 邮箱域名记忆。
//!
//! 名单导入不申请任何权限：用户从通讯录/微信/文件里复制一串姓名粘贴进来即可。
//! 姓名 -> 拼音由引擎反查（`char_to_pinyin`），之后按拼音做精确/前缀联想。

use inputx_pinyin::char_to_pinyin;
use std::sync::{Mutex, OnceLock};

fn tone_fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ā' | 'á' | 'ǎ' | 'à' => 'a',
            'ē' | 'é' | 'ě' | 'è' => 'e',
            'ī' | 'í' | 'ǐ' | 'ì' => 'i',
            'ō' | 'ó' | 'ǒ' | 'ò' => 'o',
            'ū' | 'ú' | 'ǔ' | 'ù' => 'u',
            'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' | 'ü' => 'v',
            'ń' | 'ň' => 'n',
            other => other,
        })
        .collect()
}

fn is_cjk(c: char) -> bool {
    (0x4E00..=0x9FFF).contains(&(c as u32))
}

/// 姓名 -> 拼音（去声调；多音字取首选读音）。含非中文非字母字符返回 None。
pub fn name_to_pinyin(name: &str) -> Option<String> {
    let mut out = String::new();
    for ch in name.trim().chars() {
        if is_cjk(ch) {
            let readings = char_to_pinyin(ch);
            let first = readings.first()?;
            out.push_str(&tone_fold(first).to_ascii_lowercase());
        } else if ch.is_ascii_alphabetic() {
            out.push(ch.to_ascii_lowercase());
        } else {
            return None;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// 解析粘贴文本里的姓名（换行/逗号/分号/顿号/空格分隔）。
///
/// 过滤协议分隔符（US/GS/RS）与其它控制字符：否则粘贴串可注入分隔符破坏
/// Kotlin 侧解析（`TypesakeCore.splitDelim` 会被切散）。
pub fn parse_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in
        text.split(|c: char| c.is_whitespace() || matches!(c, ',' | '，' | ';' | '；' | '、'))
    {
        let cleaned: String = piece
            .trim()
            .chars()
            .filter(|c| *c != '\u{1E}' && *c != '\u{1F}' && *c != '\u{1D}' && !c.is_control())
            .collect();
        let name = cleaned.trim();
        if name.chars().count() < 2 || name.chars().count() > 6 {
            continue;
        }
        if !out.iter().any(|x| x == name) {
            out.push(name.to_string());
        }
    }
    out
}

/// 邮箱域名记忆（用户点过哪些后缀就优先给哪些）。
fn domains() -> &'static Mutex<Vec<(String, u32)>> {
    static D: OnceLock<Mutex<Vec<(String, u32)>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn set_domains(items: Vec<(String, u32)>) {
    {
        let mut d = domains().lock().unwrap_or_else(|e| e.into_inner());
        *d = items;
        d.truncate(50);
    }
}

pub fn remember_domain(domain: &str) -> Vec<(String, u32)> {
    let d = domain.trim().to_ascii_lowercase();
    // 域名限长 64：防超长串进快照撑大 DB。
    if d.is_empty() || d.len() > 64 {
        return domains_snapshot();
    }
    // 域名只允许常规字符，其余直接拒绝（防分隔符注入）。
    if !d
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return domains_snapshot();
    }
    let out: Vec<(String, u32)>;
    {
        let mut list = domains().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(e) = list.iter_mut().find(|(x, _)| *x == d) {
            e.1 = e.1.saturating_add(1);
        } else {
            list.push((d.clone(), 1));
        }
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        list.truncate(50);
        out = list.clone();
    }
    out
}

pub fn domains_snapshot() -> Vec<(String, u32)> {
    domains().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_pinyin_and_parsing() {
        assert_eq!(name_to_pinyin("张三").as_deref(), Some("zhangsan"));
        assert_eq!(name_to_pinyin("李四").as_deref(), Some("lisi"));
        assert!(name_to_pinyin("A-1").is_none());
        let names = parse_names("张三,李四\n王五；赵六 张三");
        assert_eq!(
            names,
            vec![
                "张三".to_string(),
                "李四".to_string(),
                "王五".to_string(),
                "赵六".to_string()
            ]
        );
    }

    #[test]
    fn separators_and_control_are_filtered() {
        // 分隔符注入：粘贴含 US/GS/RS 与控制字符不得进名单。
        let names = parse_names("张三\u{1E}李四\n王\u{1F}五\u{0}");
        assert!(!names.iter().any(|n| n.contains('\u{1E}')));
        assert!(!names.iter().any(|n| n.contains('\u{1F}')));
        // 域名限长与字符白名单。
        set_domains(Vec::new());
        remember_domain(&"a".repeat(100));
        assert!(domains_snapshot().is_empty());
        remember_domain("gmail.com");
        assert_eq!(domains_snapshot().len(), 1);
        set_domains(Vec::new());
    }

    #[test]
    fn mail_domain_memory() {
        set_domains(Vec::new());
        remember_domain("Gmail.com");
        remember_domain("gmail.com");
        let d = remember_domain("qq.com");
        assert_eq!(
            d.first().map(|(x, c)| (x.as_str(), *c)),
            Some(("gmail.com", 2))
        );
        assert_eq!(domains_snapshot().len(), 2);
        set_domains(Vec::new());
    }
}
