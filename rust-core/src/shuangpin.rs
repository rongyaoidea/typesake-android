//! 双拼与大千注音：把「击键串」还原成全拼，交给既有引擎处理。
//!
//! 内置 7 套双拼方案（小鹤 / 自然码 / 微软 / 搜狗 / 智能 ABC / 小浪 / 首道）与
//! 1 套大千注音布局，全部是纯数据表，新增方案只需加一张表。
//!
//! 解码时不写「按声母类别分类」的偏好规则，而是拿引擎自带的音节表
//! （`inputx_pinyin::is_valid_syllable`）在同一个键位的多个韵母候选里，
//! 挑唯一能拼出合法音节的那个——这样换方案不必再调一套规则，
//! 也自动处理了 `g+k` 只能是 `guai`、`b+o` 只能是 `bo` 这类歧义。
//!
//! 键位表按公开键位图整理；完整性由 `round_trip_all_schemes`、
//! `zhuyin_round_trip` 自检，发布前建议再对照各方案官方键位图核对一遍。
//! 已知个别冷门方案本身自带重码（同一组击键对应两种读法），解码按表内优先级取先者，
//! 这类条目列在测试的 `KNOWN_AMBIGUOUS` 里。

use std::collections::HashMap;

/// 方案编号（0 = 关闭，即全拼）
pub const SCHEME_FLYPY: u8 = 1;
pub const SCHEME_ZIRANMA: u8 = 2;
pub const SCHEME_MICROSOFT: u8 = 3;
pub const SCHEME_SOGOU: u8 = 4;
pub const SCHEME_ABC: u8 = 5;
pub const SCHEME_XIAOLANG: u8 = 6;
pub const SCHEME_SHOUDAO: u8 = 7;
pub const SCHEME_ZHUYIN: u8 = 8;

/// 一套方案的键位表。
struct Scheme {
    name: &'static str,
    /// 占字母键的翘舌声母（键 -> 声母）；普通声母（b p m f… y w）就是字母本身，不在此列。
    digraph: &'static [(char, &'static str)],
    /// 韵母键 -> 该键可能的韵母（按表内优先级）。
    finals: &'static [(char, &'static [&'static str])],
    /// 零声母音节 -> 它的两键写法（按优先级，第一个是主写法）。
    zero: &'static [(&'static str, &'static [&'static str])],
    /// 零声母是否也接受 `o` + 韵母键（微软 / 搜狗 / 智能 ABC）。
    o_prefix: bool,
    /// `;` 是否兼作 ing 键（微软 / 搜狗）。
    semicolon: bool,
}

/// 小鹤 / 自然码 / 微软 / 搜狗 共用的翘舌声母占键。
const DIGRAPH: [(char, &str); 3] = [('v', "zh"), ('i', "ch"), ('u', "sh")];
/// 智能 ABC：a 为 zh，e 为 ch，v 为 sh。
const ABC_DIGRAPH: [(char, &str); 3] = [('a', "zh"), ('e', "ch"), ('v', "sh")];
/// 小浪：e 为 zh，i 为 ch，v 为 sh。
const XIAOLANG_DIGRAPH: [(char, &str); 3] = [('e', "zh"), ('i', "ch"), ('v', "sh")];
/// 首道：v 为 zh，i 为 ch，e 为 sh。
const SHOUDAO_DIGRAPH: [(char, &str); 3] = [('v', "zh"), ('i', "ch"), ('e', "sh")];

/// 普通声母 -> 键位（所有方案一致，只有翘舌声母换键）。
const PLAIN_INITIALS: &[(&str, char)] = &[
    ("b", 'b'),
    ("p", 'p'),
    ("m", 'm'),
    ("f", 'f'),
    ("d", 'd'),
    ("t", 't'),
    ("n", 'n'),
    ("l", 'l'),
    ("g", 'g'),
    ("k", 'k'),
    ("h", 'h'),
    ("j", 'j'),
    ("q", 'q'),
    ("x", 'x'),
    ("r", 'r'),
    ("z", 'z'),
    ("c", 'c'),
    ("s", 's'),
    ("y", 'y'),
    ("w", 'w'),
];

/// 切全拼声母用：两字声母必须排在单字声母前面，翘舌声母必须排在 z/c/s 前面。
fn initials_for(scheme: u8) -> Vec<(&'static str, char)> {
    let mut v: Vec<(&'static str, char)> = Vec::with_capacity(23);
    if let Some(sch) = table(scheme) {
        for (k, ini) in sch.digraph {
            v.push((*ini, *k));
        }
    }
    v.extend_from_slice(PLAIN_INITIALS);
    v
}

// ---------------- 七套双拼的键位表 ----------------

/// 小鹤双拼。
const XIAOHE: Scheme = Scheme {
    name: "小鹤双拼",
    digraph: &DIGRAPH,
    finals: &[
        ('q', &["iu"]),
        ('w', &["ei"]),
        ('e', &["e"]),
        ('r', &["uan"]),
        ('t', &["ve", "ue"]),
        ('y', &["un"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["ie"]),
        ('a', &["a"]),
        ('s', &["iong", "ong"]),
        ('d', &["ai"]),
        ('f', &["en"]),
        ('g', &["eng"]),
        ('h', &["ang"]),
        ('j', &["an"]),
        ('k', &["ing", "uai"]),
        ('l', &["iang", "uang"]),
        ('z', &["ou"]),
        ('x', &["ia", "ua"]),
        ('c', &["ao"]),
        ('v', &["ui", "v"]),
        ('b', &["in"]),
        ('n', &["iao"]),
        ('m', &["ian"]),
    ],
    zero: &[
        ("a", &["aa"]),
        ("ai", &["ai", "ad"]),
        ("an", &["an", "aj"]),
        ("ang", &["ah"]),
        ("ao", &["ao", "ac"]),
        ("e", &["ee"]),
        ("ei", &["ei", "ew"]),
        ("en", &["en", "ef"]),
        ("eng", &["eg"]),
        ("er", &["er"]),
        ("o", &["oo"]),
        ("ou", &["ou", "oz"]),
    ],
    o_prefix: false,
    semicolon: false,
};

/// 自然码。
const ZIRANMA: Scheme = Scheme {
    name: "自然码",
    digraph: &DIGRAPH,
    finals: &[
        ('q', &["iu"]),
        ('w', &["ia", "ua"]),
        ('e', &["e"]),
        ('r', &["uan"]),
        ('t', &["ve", "ue"]),
        ('y', &["uai", "ing"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["un"]),
        ('a', &["a"]),
        ('s', &["iong", "ong"]),
        ('d', &["iang", "uang"]),
        ('f', &["en"]),
        ('g', &["eng"]),
        ('h', &["ang"]),
        ('j', &["an"]),
        ('k', &["ao"]),
        ('l', &["ai"]),
        ('z', &["ei"]),
        ('x', &["ie"]),
        ('c', &["iao"]),
        ('v', &["ui", "v"]),
        ('b', &["ou"]),
        ('n', &["in"]),
        ('m', &["ian"]),
    ],
    zero: &[
        ("a", &["aa"]),
        ("ai", &["ai", "al"]),
        ("an", &["an", "aj"]),
        ("ang", &["ah"]),
        ("ao", &["ao", "ak"]),
        ("e", &["ee"]),
        ("ei", &["ei", "ez"]),
        ("en", &["en", "ef"]),
        ("eng", &["eg"]),
        ("er", &["er"]),
        ("o", &["oo"]),
        ("ou", &["ou", "ob"]),
    ],
    o_prefix: false,
    semicolon: false,
};

/// 微软 / 搜狗共用的零声母写法：`o` 加韵母键，`a` / `e` 开头的也接受双写元音。
const O_PREFIX_ZERO: &[(&str, &[&str])] = &[
    ("a", &["oa", "aa"]),
    ("ai", &["ol", "al"]),
    ("an", &["oj", "aj"]),
    ("ang", &["oh", "ah"]),
    ("ao", &["ok", "ak"]),
    ("e", &["oe", "ee"]),
    ("ei", &["oz", "ez"]),
    ("en", &["of", "ef"]),
    ("eng", &["og", "eg"]),
    ("er", &["or", "er"]),
    ("o", &["oo"]),
    ("ou", &["ob", "ou"]),
];

/// 微软双拼：ü 与 üe 都在 `v` 侧的写法与搜狗略有差别，ing 放在 `;`。
const MICROSOFT: Scheme = Scheme {
    name: "微软双拼",
    digraph: &DIGRAPH,
    finals: &[
        ('q', &["iu"]),
        ('w', &["ia", "ua"]),
        ('e', &["e"]),
        ('r', &["uan"]),
        ('t', &["ve", "ue"]),
        ('y', &["uai", "v"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["un"]),
        ('a', &["a"]),
        ('s', &["iong", "ong"]),
        ('d', &["iang", "uang"]),
        ('f', &["en"]),
        ('g', &["eng"]),
        ('h', &["ang"]),
        ('j', &["an"]),
        ('k', &["ao"]),
        ('l', &["ai"]),
        (';', &["ing"]),
        ('z', &["ei"]),
        ('x', &["ie"]),
        ('c', &["iao"]),
        ('v', &["ui", "ve", "ue"]),
        ('b', &["ou"]),
        ('n', &["in"]),
        ('m', &["ian"]),
    ],
    zero: O_PREFIX_ZERO,
    o_prefix: true,
    semicolon: true,
};

/// 搜狗双拼：与微软只差 `v` 键不兼作 üe。
const SOGOU: Scheme = Scheme {
    name: "搜狗双拼",
    digraph: &DIGRAPH,
    finals: &[
        ('q', &["iu"]),
        ('w', &["ia", "ua"]),
        ('e', &["e"]),
        ('r', &["uan"]),
        ('t', &["ve", "ue"]),
        ('y', &["uai", "v"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["un"]),
        ('a', &["a"]),
        ('s', &["iong", "ong"]),
        ('d', &["iang", "uang"]),
        ('f', &["en"]),
        ('g', &["eng"]),
        ('h', &["ang"]),
        ('j', &["an"]),
        ('k', &["ao"]),
        ('l', &["ai"]),
        (';', &["ing"]),
        ('z', &["ei"]),
        ('x', &["ie"]),
        ('c', &["iao"]),
        ('v', &["ui"]),
        ('b', &["ou"]),
        ('n', &["in"]),
        ('m', &["ian"]),
    ],
    zero: O_PREFIX_ZERO,
    o_prefix: true,
    semicolon: true,
};

/// 智能 ABC 的零声母只认 `o` 前缀：`aa` / `ee` 在这套方案里是 zha / che。
const ABC_ZERO: &[(&str, &[&str])] = &[
    ("a", &["oa"]),
    ("ai", &["ol"]),
    ("an", &["oj"]),
    ("ang", &["oh"]),
    ("ao", &["ok"]),
    ("e", &["oe"]),
    ("ei", &["oq"]),
    ("en", &["of"]),
    ("eng", &["og"]),
    ("er", &["or"]),
    ("o", &["oo"]),
    ("ou", &["ob"]),
];

/// 智能 ABC：翘舌声母在 `a` / `e` / `v`。
const ABC: Scheme = Scheme {
    name: "智能ABC",
    digraph: &ABC_DIGRAPH,
    finals: &[
        ('q', &["ei"]),
        ('w', &["ian"]),
        ('e', &["e"]),
        ('r', &["iu"]),
        ('t', &["iang", "uang"]),
        ('y', &["ing"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["uan"]),
        ('a', &["a"]),
        ('s', &["iong", "ong"]),
        ('d', &["ia", "ua"]),
        ('f', &["en"]),
        ('g', &["eng"]),
        ('h', &["ang"]),
        ('j', &["an"]),
        ('k', &["ao"]),
        ('l', &["ai"]),
        ('z', &["iao"]),
        ('x', &["ie"]),
        ('c', &["in", "uai"]),
        ('v', &["v"]),
        ('b', &["ou"]),
        ('n', &["un"]),
        ('m', &["ui", "ve", "ue"]),
    ],
    zero: ABC_ZERO,
    o_prefix: true,
    semicolon: false,
};

/// 小浪双拼：x 为 ü / u，v 兼作 uai 与 ing，零声母的 e 一族换到 `u` 引导。
const XIAOLANG: Scheme = Scheme {
    name: "小浪双拼",
    digraph: &XIAOLANG_DIGRAPH,
    finals: &[
        ('w', &["ei"]),
        ('e', &["e"]),
        ('r', &["ou"]),
        ('t', &["iu"]),
        ('y', &["un", "vn"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["ie"]),
        ('a', &["a"]),
        ('s', &["ao"]),
        ('d', &["ui", "in"]),
        ('f', &["ian", "ua"]),
        ('g', &["uan"]),
        ('h', &["ang"]),
        ('j', &["an", "iong"]),
        ('k', &["ai", "ia"]),
        ('l', &["ong"]),
        ('z', &["uang"]),
        ('x', &["v", "u"]),
        ('c', &["iao"]),
        ('v', &["uai", "ing"]),
        ('b', &["ve", "ue"]),
        ('n', &["eng"]),
        ('m', &["iang", "en"]),
    ],
    zero: &[
        ("a", &["aa"]),
        ("ai", &["ai"]),
        ("an", &["an"]),
        ("ang", &["ah"]),
        ("ao", &["ao"]),
        ("e", &["uu"]),
        ("ei", &["ui"]),
        ("en", &["un"]),
        ("eng", &["un"]),
        ("er", &["ur"]),
        ("o", &["oo"]),
        ("ou", &["ou"]),
    ],
    o_prefix: false,
    semicolon: false,
};

/// 首道双拼零声母：a / o 开头照全拼双写，`e` 键让给了 sh，所以 e / ei / eng 改用 `u` 引导。
const SHOUDAO_ZERO: &[(&str, &[&str])] = &[
    ("a", &["aa"]),
    ("ai", &["ai"]),
    ("an", &["an"]),
    ("ang", &["ay"]),
    ("ao", &["ao"]),
    ("e", &["ue"]),
    ("ei", &["ui"]),
    ("en", &["en"]),
    ("eng", &["uf"]),
    ("er", &["er"]),
    ("o", &["oo"]),
    ("ou", &["ou"]),
];

/// 首道双拼：ue（jue / que / xue / yue）在 `l`，üe（lve / nve）单独在 `b`。
const SHOUDAO: Scheme = Scheme {
    name: "首道双拼",
    digraph: &SHOUDAO_DIGRAPH,
    finals: &[
        ('q', &["iu"]),
        ('w', &["ua"]),
        ('e', &["e"]),
        ('r', &["ie"]),
        ('t', &["uan"]),
        ('y', &["ang"]),
        ('u', &["u"]),
        ('i', &["i"]),
        ('o', &["uo", "o"]),
        ('p', &["iao"]),
        ('a', &["a"]),
        ('s', &["ou"]),
        ('d', &["ao"]),
        ('f', &["eng"]),
        ('g', &["uai", "ing"]),
        ('h', &["ong", "iong"]),
        ('j', &["an"]),
        ('k', &["en", "ia"]),
        ('l', &["ai", "ue"]),
        ('z', &["un"]),
        ('x', &["iang", "uang"]),
        ('c', &["in"]),
        ('v', &["ui", "v"]),
        ('b', &["ve"]),
        ('n', &["ian"]),
        ('m', &["ei"]),
    ],
    zero: SHOUDAO_ZERO,
    o_prefix: false,
    semicolon: false,
};

/// 方案编号 -> 键位表（0 = 全拼，没有表）。
fn table(scheme: u8) -> Option<&'static Scheme> {
    match scheme {
        SCHEME_FLYPY => Some(&XIAOHE),
        SCHEME_ZIRANMA => Some(&ZIRANMA),
        SCHEME_MICROSOFT => Some(&MICROSOFT),
        SCHEME_SOGOU => Some(&SOGOU),
        SCHEME_ABC => Some(&ABC),
        SCHEME_XIAOLANG => Some(&XIAOLANG),
        SCHEME_SHOUDAO => Some(&SHOUDAO),
        _ => None,
    }
}

impl Scheme {
    /// 某个键位可能对应的韵母（未知键返回空表）。
    fn finals_of(&self, key: char) -> &'static [&'static str] {
        self.finals
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, f)| *f)
            .unwrap_or(&[])
    }

    /// 韵母 -> 键位（反查，取表内第一个命中的键）。
    fn key_of_final(&self, final_: &str) -> Option<char> {
        self.finals
            .iter()
            .find(|(_, fs)| fs.contains(&final_))
            .map(|(k, _)| *k)
    }

    /// 某个键位对应的翘舌声母。
    fn digraph_of(&self, key: char) -> Option<&'static str> {
        self.digraph
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, i)| *i)
    }

    /// 零声母的两键写法反查：(首键, 次键) -> 音节。
    fn zero_syllable(&self, first: char, second: char) -> Option<String> {
        let pair = [first, second];
        let two: String = pair.iter().collect();
        self.zero
            .iter()
            .find(|(_, keys)| keys.iter().any(|k| *k == two))
            .map(|(syl, _)| (*syl).to_string())
    }

    /// 零声母音节的首选两键写法。
    fn zero_keys(&self, syllable: &str) -> Option<String> {
        self.zero
            .iter()
            .find(|(syl, _)| *syl == syllable)
            .and_then(|(_, keys)| keys.first().map(|k| (*k).to_string()))
    }
}

/// 该方案是否把 `;` 当作 ing 键（引擎规整输入时用）。
pub fn uses_semicolon(scheme: u8) -> bool {
    table(scheme).map(|s| s.semicolon).unwrap_or(false)
}

/// 所有方案，供设置页遍历。
pub fn schemes() -> &'static [(u8, &'static str)] {
    &[
        (0, "全拼"),
        (SCHEME_FLYPY, "小鹤双拼"),
        (SCHEME_ZIRANMA, "自然码"),
        (SCHEME_MICROSOFT, "微软双拼"),
        (SCHEME_SOGOU, "搜狗双拼"),
        (SCHEME_ABC, "智能ABC"),
        (SCHEME_XIAOLANG, "小浪双拼"),
        (SCHEME_SHOUDAO, "首道双拼"),
        (SCHEME_ZHUYIN, "大千注音"),
    ]
}

/// 设置页展示用的方案名。
pub fn scheme_name(scheme: u8) -> &'static str {
    if let Some(sch) = table(scheme) {
        return sch.name;
    }
    schemes()
        .iter()
        .find(|(id, _)| *id == scheme)
        .map(|(_, n)| *n)
        .unwrap_or("全拼")
}

// ---------------- 解码：击键 -> 全拼 ----------------

/// 规整：保留该方案会用到的字符并转小写
/// （全拼只留字母；微软/搜狗额外留 `;`；注音留数字与 `;`，它们是大千布局的一部分）。
fn sanitize(input: &str, scheme: u8) -> String {
    let keep_semi = uses_semicolon(scheme) || scheme == SCHEME_ZHUYIN;
    let keep_digit = scheme == SCHEME_ZHUYIN;
    input
        .chars()
        .filter(|c| {
            c.is_ascii_alphabetic()
                || (keep_semi && *c == ';')
                || (keep_digit && c.is_ascii_digit())
        })
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// 在同一键位的多个韵母候选里，挑唯一能和声母拼出合法音节的那个。
///
/// 一个都拼不出时返回 `None`——调用方据此改走零声母分支。
/// 这不是防御性设计：首道双拼把 `e` 让给了 sh，`en` / `er` 却仍按全拼敲，
/// 正是因为 `sh+ian`、`sh+ie` 都不是合法音节，才不会和零声母撞键。
fn pick_final(sch: &Scheme, key: char, initial: &str) -> Option<String> {
    let cands = sch.finals_of(key);
    let mut probe = String::with_capacity(initial.len() + 6);
    for f in cands {
        probe.clear();
        probe.push_str(initial);
        probe.push_str(f);
        if inputx_pinyin::is_valid_syllable(&probe) {
            return Some((*f).to_string());
        }
    }
    None
}

/// 零声母的「首字母引导」：次键解出的韵母以首键开头且本身是合法音节。
fn guide_zero(sch: &Scheme, first: char, second: char) -> Option<String> {
    sch.finals_of(second)
        .iter()
        .find(|f| f.starts_with(first) && inputx_pinyin::is_valid_syllable(f))
        .map(|f| (*f).to_string())
}

/// `o` 前缀型零声母：次键解出的第一个合法音节。
fn o_prefix_zero(sch: &Scheme, second: char) -> Option<String> {
    sch.finals_of(second)
        .iter()
        .find(|f| inputx_pinyin::is_valid_syllable(f))
        .map(|f| (*f).to_string())
}

/// 双拼击键串 -> 全拼串。无法完整还原的片段原样保留（不丢用户输入）。
pub fn to_full(input: &str, scheme: u8) -> String {
    if scheme == SCHEME_ZHUYIN {
        return zhuyin_to_full(input);
    }
    let s = sanitize(input, scheme);
    let Some(sch) = table(scheme) else {
        return s;
    };
    if s.is_empty() {
        return s;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() * 2);
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];

        // 元音键（同时也是各方案的翘舌键位）：先试翘舌，再试零声母，最后落单原样保留
        if matches!(c, 'a' | 'e' | 'o' | 'i' | 'u' | 'v') {
            // 1) 该键兼作翘舌声母，且下一键能拼出合法音节 -> 就按翘舌读
            if let Some(ini) = sch.digraph_of(c) {
                if i + 1 < chars.len() {
                    if let Some(f) = pick_final(sch, chars[i + 1], ini) {
                        out.push_str(ini);
                        out.push_str(&f);
                        i += 2;
                        continue;
                    }
                }
                // 拼不出合法音节：落到 2)，这正是「e 键既是 sh 又能起零声母」的消歧点
            }
            // 2) 零声母：显式两键表 -> 首字母引导 -> o 前缀
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if let Some(syl) = sch.zero_syllable(c, next) {
                    out.push_str(&syl);
                    i += 2;
                    continue;
                }
                if let Some(syl) = guide_zero(sch, c, next) {
                    out.push_str(&syl);
                    i += 2;
                    continue;
                }
                if sch.o_prefix && c == 'o' {
                    if let Some(syl) = o_prefix_zero(sch, next) {
                        out.push_str(&syl);
                        i += 2;
                        continue;
                    }
                }
            }
            // 3) 落单：原样保留，交由引擎自己决定有没有候选
            out.push(c);
            i += 1;
            continue;
        }

        // 4) 普通声母（含 y / w）
        let initial = c.to_string();
        i += 1;
        if i < chars.len() {
            if let Some(f) = pick_final(sch, chars[i], &initial) {
                out.push_str(&initial);
                out.push_str(&f);
                i += 1;
                continue;
            }
        }
        out.push_str(&initial);
    }
    out
}

// ---------------- 编码：全拼 -> 击键 ----------------

/// 全拼音节 -> 某套方案的击键（用于测试与反向校验）。
pub fn encode_with(syllable: &str, scheme: u8) -> Option<String> {
    if scheme == SCHEME_ZHUYIN {
        return zhuyin_keys(syllable);
    }
    let Some(sch) = table(scheme) else {
        // 全拼没有表：原样
        let s = syllable.trim();
        return (!s.is_empty()).then(|| s.to_ascii_lowercase());
    };
    let s = syllable.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    // 零声母优先查表（能拿到该方案认可的主写法）
    if let Some(keys) = sch.zero_keys(&s) {
        return Some(keys);
    }
    for (ini, ikey) in initials_for(scheme) {
        if let Some(rest) = s.strip_prefix(ini) {
            if rest.is_empty() {
                return Some(ikey.to_string());
            }
            return sch.key_of_final(rest).map(|fk| format!("{ikey}{fk}"));
        }
    }
    // 表外的零声母（如 a/o/e 开头）：首字母 + 韵母键
    let first = s.chars().next()?;
    if matches!(first, 'a' | 'o' | 'e') {
        return sch.key_of_final(&s).map(|fk| format!("{first}{fk}"));
    }
    Some(first.to_string())
}

/// 全拼音节 -> 小鹤击键（默认方案）。
pub fn encode(syllable: &str) -> Option<String> {
    encode_with(syllable, SCHEME_FLYPY)
}

/// 反查表：键位 -> 韵母（供测试/调试；返回指定方案的表）。
pub fn debug_final_map(scheme: u8) -> HashMap<char, Vec<&'static str>> {
    let mut m: HashMap<char, Vec<&'static str>> = HashMap::new();
    if let Some(sch) = table(scheme) {
        for (k, fs) in sch.finals {
            m.entry(*k).or_default().extend(fs.iter().copied());
        }
    }
    m
}

// ---------------- 大千注音 ----------------

/// 大千布局：ASCII 键 -> 注音符号。
const ZHUYIN_KEYS: &[(char, char)] = &[
    // 聲母
    ('1', 'ㄅ'),
    ('q', 'ㄆ'),
    ('a', 'ㄇ'),
    ('z', 'ㄈ'),
    ('2', 'ㄉ'),
    ('w', 'ㄊ'),
    ('s', 'ㄋ'),
    ('x', 'ㄌ'),
    ('e', 'ㄍ'),
    ('d', 'ㄎ'),
    ('c', 'ㄏ'),
    ('r', 'ㄐ'),
    ('f', 'ㄑ'),
    ('v', 'ㄒ'),
    ('5', 'ㄓ'),
    ('t', 'ㄔ'),
    ('g', 'ㄕ'),
    ('b', 'ㄖ'),
    ('y', 'ㄗ'),
    ('h', 'ㄘ'),
    ('n', 'ㄙ'),
    // 介音
    ('u', 'ㄧ'),
    ('j', 'ㄨ'),
    ('m', 'ㄩ'),
    // 韻母
    ('8', 'ㄚ'),
    ('i', 'ㄛ'),
    ('k', 'ㄜ'),
    (',', 'ㄝ'),
    ('9', 'ㄞ'),
    ('o', 'ㄟ'),
    ('l', 'ㄠ'),
    ('.', 'ㄡ'),
    ('0', 'ㄢ'),
    ('p', 'ㄣ'),
    (';', 'ㄤ'),
    ('/', 'ㄥ'),
    ('-', 'ㄦ'),
    // 聲調
    ('6', 'ˊ'),
    ('3', 'ˇ'),
    ('4', 'ˋ'),
    ('7', '˙'),
];

/// 声调键：不影响音节，解码前直接丢掉（第一声用空格，也不进击键串）。
const TONE_KEYS: &[char] = &['6', '3', '4', '7'];

/// y 开头的音节（拼写上是 `i` / `ü` 的字头写法），逐个给注音。
const Y_SYLLABLES: &[(&str, &str)] = &[
    ("yi", "ㄧ"),
    ("ya", "ㄧㄚ"),
    ("yao", "ㄧㄠ"),
    ("ye", "ㄧㄝ"),
    ("you", "ㄧㄡ"),
    ("yan", "ㄧㄢ"),
    ("yin", "ㄧㄣ"),
    ("yang", "ㄧㄤ"),
    ("ying", "ㄧㄥ"),
    ("yong", "ㄩㄥ"),
    ("yu", "ㄩ"),
    ("yue", "ㄩㄝ"),
    ("yuan", "ㄩㄢ"),
    ("yun", "ㄩㄣ"),
];

/// w 开头的音节（拼写上是 `u` 的字头写法）。
const W_SYLLABLES: &[(&str, &str)] = &[
    ("wu", "ㄨ"),
    ("wa", "ㄨㄚ"),
    ("wo", "ㄨㄛ"),
    ("wai", "ㄨㄞ"),
    ("wei", "ㄨㄟ"),
    ("wan", "ㄨㄢ"),
    ("wen", "ㄨㄣ"),
    ("wang", "ㄨㄤ"),
    ("weng", "ㄨㄥ"),
];

/// 零声母音节 -> 注音。
const ZERO_SYLLABLES: &[(&str, &str)] = &[
    ("a", "ㄚ"),
    ("ai", "ㄞ"),
    ("an", "ㄢ"),
    ("ang", "ㄤ"),
    ("ao", "ㄠ"),
    ("e", "ㄜ"),
    ("ei", "ㄟ"),
    ("en", "ㄣ"),
    ("eng", "ㄥ"),
    ("er", "ㄦ"),
    ("o", "ㄛ"),
    ("ou", "ㄡ"),
];

/// 声母 -> 注音符号。
const ZHUYIN_INITIALS: &[(&str, char)] = &[
    ("zh", 'ㄓ'),
    ("ch", 'ㄔ'),
    ("sh", 'ㄕ'),
    ("b", 'ㄅ'),
    ("p", 'ㄆ'),
    ("m", 'ㄇ'),
    ("f", 'ㄈ'),
    ("d", 'ㄉ'),
    ("t", 'ㄊ'),
    ("n", 'ㄋ'),
    ("l", 'ㄌ'),
    ("g", 'ㄍ'),
    ("k", 'ㄎ'),
    ("h", 'ㄏ'),
    ("j", 'ㄐ'),
    ("q", 'ㄑ'),
    ("x", 'ㄒ'),
    ("r", 'ㄖ'),
    ("z", 'ㄗ'),
    ("c", 'ㄘ'),
    ("s", 'ㄙ'),
];

/// 韵母 -> 注音（含介音）。j/q/x 与 y 的 ü 系写法由调用方单独覆盖。
const ZHUYIN_FINALS: &[(&str, &str)] = &[
    ("a", "ㄚ"),
    ("o", "ㄛ"),
    ("e", "ㄜ"),
    ("i", "ㄧ"),
    ("u", "ㄨ"),
    ("v", "ㄩ"),
    ("ai", "ㄞ"),
    ("ei", "ㄟ"),
    ("ao", "ㄠ"),
    ("ou", "ㄡ"),
    ("an", "ㄢ"),
    ("en", "ㄣ"),
    ("ang", "ㄤ"),
    ("eng", "ㄥ"),
    ("er", "ㄦ"),
    ("ia", "ㄧㄚ"),
    ("ie", "ㄧㄝ"),
    ("iao", "ㄧㄠ"),
    ("iu", "ㄧㄡ"),
    ("ian", "ㄧㄢ"),
    ("in", "ㄧㄣ"),
    ("iang", "ㄧㄤ"),
    ("ing", "ㄧㄥ"),
    ("iong", "ㄩㄥ"),
    ("ua", "ㄨㄚ"),
    ("uo", "ㄨㄛ"),
    ("uai", "ㄨㄞ"),
    ("ui", "ㄨㄟ"),
    ("uan", "ㄨㄢ"),
    ("un", "ㄨㄣ"),
    ("uang", "ㄨㄤ"),
    ("ong", "ㄨㄥ"),
    ("ue", "ㄩㄝ"),
    ("ve", "ㄩㄝ"),
    ("van", "ㄩㄢ"),
    ("vn", "ㄩㄣ"),
];

/// 翘舌 + 独韵母 `i` 是舌尖元音，注音里不写符号（zhi=ㄓ、zi=ㄗ）。
const APICAL: &[&str] = &["zh", "ch", "sh", "r", "z", "c", "s"];

/// j / q / x 后面的 `u` 实际是 ü，注音要写 `ㄩ`。
const JQX_UE: &[(&str, &str)] = &[
    ("u", "ㄩ"),
    ("ue", "ㄩㄝ"),
    ("uan", "ㄩㄢ"),
    ("un", "ㄩㄣ"),
    ("iong", "ㄩㄥ"),
];

/// 全拼音节 -> 注音符号串（拿不到就返回 None）。
fn pinyin_to_zhuyin(syllable: &str) -> Option<String> {
    let s = syllable.trim().to_ascii_lowercase();
    if let Some(z) = Y_SYLLABLES.iter().find(|(p, _)| *p == s) {
        return Some(z.1.to_string());
    }
    if let Some(z) = W_SYLLABLES.iter().find(|(p, _)| *p == s) {
        return Some(z.1.to_string());
    }
    if let Some(z) = ZERO_SYLLABLES.iter().find(|(p, _)| *p == s) {
        return Some(z.1.to_string());
    }
    // 切最长声母（两字优先）
    let mut rest = "";
    let mut initial = "";
    for (ini, _) in ZHUYIN_INITIALS {
        if let Some(r) = s.strip_prefix(ini) {
            initial = ini;
            rest = r;
            break;
        }
    }
    if initial.is_empty() {
        return None;
    }
    if APICAL.contains(&initial) && rest == "i" {
        return Some(symbol_of(initial)?.to_string());
    }
    if matches!(initial, "j" | "q" | "x") {
        if let Some(z) = JQX_UE.iter().find(|(f, _)| *f == rest) {
            return Some(format!("{}{}", symbol_of(initial)?, z.1));
        }
    }
    let tail = ZHUYIN_FINALS.iter().find(|(f, _)| *f == rest)?.1;
    Some(format!("{}{}", symbol_of(initial)?, tail))
}

fn symbol_of(initial: &str) -> Option<char> {
    ZHUYIN_INITIALS
        .iter()
        .find(|(i, _)| *i == initial)
        .map(|(_, c)| *c)
}

/// 「全拼音节 -> 注音串」表（进程内构造一次）。
fn zhuyin_index() -> &'static HashMap<String, String> {
    use std::sync::OnceLock;
    static IDX: OnceLock<HashMap<String, String>> = OnceLock::new();
    IDX.get_or_init(|| {
        let initials = [
            "", "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q",
            "x", "r", "z", "c", "s",
        ];
        let finals = [
            "a", "o", "e", "i", "u", "v", "ai", "ei", "ao", "ou", "an", "en", "ang", "eng", "er",
            "ia", "ie", "iao", "iu", "ian", "in", "iang", "ing", "iong", "ua", "uo", "uai", "ui",
            "uan", "un", "uang", "ong", "ue", "ve",
        ];
        let mut m: HashMap<String, String> = HashMap::new();
        // 先放零声母，再放 initial x final 的枚举，最后放 y / w 字头写法（拼写上是 i / ü / u
        // 的变体，遇到同形符号以它们为准）
        for (p, z) in ZERO_SYLLABLES {
            m.insert((*z).to_string(), (*p).to_string());
        }
        for ini in initials {
            for f in finals {
                let cand = format!("{ini}{f}");
                if inputx_pinyin::is_valid_syllable(&cand) {
                    if let Some(z) = pinyin_to_zhuyin(&cand) {
                        m.insert(z, cand);
                    }
                }
            }
        }
        for (p, z) in Y_SYLLABLES.iter().chain(W_SYLLABLES) {
            m.insert((*z).to_string(), (*p).to_string());
        }
        m
    })
}

/// 大千击键 -> 全拼。声调键丢弃，最长匹配切音节，认不出的原样保留。
fn zhuyin_to_full(input: &str) -> String {
    let idx = zhuyin_index();
    let mut symbols = String::with_capacity(input.len());
    for ch in input.chars() {
        if TONE_KEYS.contains(&ch) {
            // 声调不影响音节切分，直接丢掉
            continue;
        }
        if let Some(sym) = ZHUYIN_KEYS.iter().find(|(k, _)| *k == ch).map(|(_, s)| *s) {
            symbols.push(sym);
        } else if ch.is_ascii_alphabetic() {
            symbols.push(ch.to_ascii_lowercase());
        }
    }
    let chars: Vec<char> = symbols.chars().collect();
    let mut out = String::with_capacity(symbols.len() * 2);
    let mut i = 0usize;
    while i < chars.len() {
        let mut hit = false;
        // 最长优先：一次最多吃 4 个符号（注音音节最多 聲母 + 介音 + 韻母 + 聲調，声调已丢）
        for len in (1..=4.min(chars.len() - i)).rev() {
            let cand: String = chars[i..i + len].iter().collect();
            if let Some(pinyin) = idx.get(&cand) {
                out.push_str(pinyin);
                i += len;
                hit = true;
                break;
            }
        }
        if !hit {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// 全拼音节 -> 大千击键。
fn zhuyin_keys(syllable: &str) -> Option<String> {
    let z = pinyin_to_zhuyin(syllable)?;
    let mut out = String::with_capacity(z.chars().count());
    for sym in z.chars() {
        out.push(
            ZHUYIN_KEYS
                .iter()
                .find(|(_, s)| *s == sym)
                .map(|(k, _)| *k)?,
        );
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 枚举引擎认的所有合法音节（380 个左右），当作往返测试的样本集。
    fn all_syllables() -> Vec<String> {
        let initials = [
            "", "zh", "ch", "sh", "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q",
            "x", "r", "z", "c", "s",
        ];
        let finals = [
            "a", "o", "e", "i", "u", "v", "ai", "ei", "ao", "ou", "an", "en", "ang", "eng", "er",
            "ia", "ie", "iao", "iu", "ian", "in", "iang", "ing", "iong", "ua", "uo", "uai", "ui",
            "uan", "un", "uang", "ong", "ue", "ve",
        ];
        let mut v: Vec<String> = Vec::new();
        for i in initials {
            for f in finals {
                let s = format!("{i}{f}");
                if inputx_pinyin::is_valid_syllable(&s) {
                    v.push(s);
                }
            }
        }
        for (p, _) in Y_SYLLABLES.iter().chain(W_SYLLABLES) {
            v.push((*p).to_string());
        }
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn encodes_common_syllables() {
        assert_eq!(encode("ni").as_deref(), Some("ni"));
        assert_eq!(encode("hao").as_deref(), Some("hc"));
        assert_eq!(encode("zhong").as_deref(), Some("vs"));
        assert_eq!(encode("guo").as_deref(), Some("go"));
        assert_eq!(encode("xue").as_deref(), Some("xt"));
        assert_eq!(encode("shang").as_deref(), Some("uh"));
        assert_eq!(encode("ai").as_deref(), Some("ai"));
        assert_eq!(encode("ang").as_deref(), Some("ah"));
        assert_eq!(encode("an").as_deref(), Some("an"));
        assert_eq!(encode("nv").as_deref(), Some("nv"));
    }

    /// 键位表里自带的重码（同一组击键对应两种读法）：解码只能取表内先者，
    /// 因此另一半音节往返时必然对不上，单独列出来而不是让测试糊过去。
    /// 第 0 位是方案编号，第 1 位是拼不通的那个音节。
    const KNOWN_AMBIGUOUS: &[(u8, &str)] = &[
        // 小浪：lk 同时是 lai / lia，公开键位表本就重码，解码取先者 lai
        (SCHEME_XIAOLANG, "lia"),
        // 小浪：nm 同时是 nen / niang，解码取先者 niang
        (SCHEME_XIAOLANG, "nen"),
        // 小浪：en / eng 共用 un，解码取先者 en，eng 只能靠前缀补全命中
        (SCHEME_XIAOLANG, "eng"),
    ];

    #[test]
    fn round_trip_all_schemes() {
        let syllables = all_syllables();
        assert!(syllables.len() > 350, "音节样本太少：{}", syllables.len());
        for scheme in 1..=7u8 {
            let mut bad: Vec<String> = Vec::new();
            for syl in &syllables {
                if KNOWN_AMBIGUOUS.contains(&(scheme, syl.as_str())) {
                    continue;
                }
                match encode_with(syl, scheme) {
                    Some(code) => {
                        let back = to_full(&code, scheme);
                        if back != *syl {
                            bad.push(format!("{syl} -> {code} -> {back}"));
                        }
                    }
                    None => bad.push(format!("{syl} 无编码")),
                }
            }
            assert!(
                bad.is_empty(),
                "{} 往返不一致（{} 条）：{:#?}",
                scheme_name(scheme),
                bad.len(),
                bad
            );
        }
    }

    #[test]
    fn sentence_level_conversion() {
        // 你好 -> ni + hc；中国 -> vs + go；学习 -> xtxi
        assert_eq!(to_full("nihc", SCHEME_FLYPY), "nihao");
        assert_eq!(to_full("vsgo", SCHEME_FLYPY), "zhongguo");
        assert_eq!(to_full("xtxi", SCHEME_FLYPY), "xuexi");
        // 自然码 / 微软 / 搜狗：zh=v、ong=s、g+uo=go
        assert_eq!(to_full("vsgo", SCHEME_ZIRANMA), "zhongguo");
        assert_eq!(to_full("vsgo", SCHEME_MICROSOFT), "zhongguo");
        assert_eq!(to_full("vsgo", SCHEME_SOGOU), "zhongguo");
        // 智能 ABC：zh=a、ong=s、g+uo=go
        assert_eq!(to_full("asgo", SCHEME_ABC), "zhongguo");
        // 小浪：zh=e、ong=l、g+uo=go
        assert_eq!(to_full("elgo", SCHEME_XIAOLANG), "zhongguo");
        // 首道：zh=v、ong=h、g+uo=go
        assert_eq!(to_full("vhgo", SCHEME_SHOUDAO), "zhongguo");
        // 微软双拼把 ing 放在 ; 上，小鹤没有这个键
        assert_eq!(to_full("b;", SCHEME_MICROSOFT), "bing");
        assert_eq!(to_full("b;", SCHEME_FLYPY), "b");
    }

    #[test]
    fn scheme_names_and_features() {
        assert_eq!(scheme_name(0), "全拼");
        assert_eq!(scheme_name(SCHEME_ZHUYIN), "大千注音");
        assert_eq!(schemes().len(), 9);
        assert!(uses_semicolon(SCHEME_MICROSOFT));
        assert!(uses_semicolon(SCHEME_SOGOU));
        assert!(!uses_semicolon(SCHEME_FLYPY));
        assert!(!uses_semicolon(SCHEME_ZHUYIN));
    }

    #[test]
    fn semicolon_is_ing_only_where_supported() {
        // 微软/搜狗：x + ; = xing
        assert_eq!(to_full("x;", SCHEME_MICROSOFT), "xing");
        assert_eq!(to_full("x;", SCHEME_SOGOU), "xing");
        // 小鹤没有 ; 键：; 被规整掉
        assert_eq!(to_full("x;", SCHEME_FLYPY), "x");
        // 清屏不影响
        assert_eq!(to_full("X;", SCHEME_MICROSOFT), "xing");
    }

    #[test]
    fn zero_initials_decode_correctly() {
        // 小鹤：爱=ad 安=aj 昂=ah 欧=oz 二=er 哦=oo 恶=ee
        assert_eq!(to_full("ad", SCHEME_FLYPY), "ai");
        assert_eq!(to_full("aj", SCHEME_FLYPY), "an");
        assert_eq!(to_full("ah", SCHEME_FLYPY), "ang");
        assert_eq!(to_full("oz", SCHEME_FLYPY), "ou");
        assert_eq!(to_full("er", SCHEME_FLYPY), "er");
        assert_eq!(to_full("oo", SCHEME_FLYPY), "o");
        assert_eq!(to_full("ee", SCHEME_FLYPY), "e");
        // 微软：按=oj 欧=ob 二=or 爱=ol
        assert_eq!(to_full("oj", SCHEME_MICROSOFT), "an");
        assert_eq!(to_full("ob", SCHEME_MICROSOFT), "ou");
        assert_eq!(to_full("or", SCHEME_MICROSOFT), "er");
        assert_eq!(to_full("ol", SCHEME_MICROSOFT), "ai");
        // 智能 ABC：按=oj，aa 必须是 zha（不是零声母 a）
        assert_eq!(to_full("oj", SCHEME_ABC), "an");
        assert_eq!(to_full("aa", SCHEME_ABC), "zha");
        // 首道：昂=ay 二=er
        assert_eq!(to_full("ay", SCHEME_SHOUDAO), "ang");
    }

    #[test]
    fn disabled_scheme_is_passthrough() {
        assert_eq!(to_full("nihao", 0), "nihao");
        assert_eq!(to_full("Nihao APP", 0), "nihaoapp");
    }

    #[test]
    fn partial_input_is_kept_not_dropped() {
        // 落单的键原样保留（不再硬凑成翘舌声母），交由引擎自己决定有没有候选
        assert_eq!(to_full("u", SCHEME_FLYPY), "u");
        assert_eq!(to_full("v", SCHEME_FLYPY), "v");
        assert_eq!(to_full("n", SCHEME_FLYPY), "n");
        // 首道的 e 既是 sh 又能起零声母：sh+ian 拼不出合法音节，于是回落到 en
        assert_eq!(to_full("en", SCHEME_SHOUDAO), "en");
        assert_eq!(to_full("er", SCHEME_SHOUDAO), "er");
        // 而能拼通的 ee 仍按 sh 读
        assert_eq!(to_full("ee", SCHEME_SHOUDAO), "she");
        // 认不出的符号不参与解码
        assert_eq!(to_full("ni3", SCHEME_FLYPY), "ni");
    }

    #[test]
    fn zhuyin_known_symbols() {
        assert_eq!(pinyin_to_zhuyin("xiong"), Some("ㄒㄩㄥ".into()));
        assert_eq!(pinyin_to_zhuyin("xiu"), Some("ㄒㄧㄡ".into()));
        assert_eq!(pinyin_to_zhuyin("xue"), Some("ㄒㄩㄝ".into()));
        assert_eq!(pinyin_to_zhuyin("gong"), Some("ㄍㄨㄥ".into()));
        assert_eq!(pinyin_to_zhuyin("feng"), Some("ㄈㄥ".into()));
        assert_eq!(pinyin_to_zhuyin("ni"), Some("ㄋㄧ".into()));
        assert_eq!(pinyin_to_zhuyin("lv"), Some("ㄌㄩ".into()));
        assert_eq!(pinyin_to_zhuyin("ju"), Some("ㄐㄩ".into()));
        assert_eq!(pinyin_to_zhuyin("you"), Some("ㄧㄡ".into()));
        assert_eq!(pinyin_to_zhuyin("er"), Some("ㄦ".into()));
        assert_eq!(pinyin_to_zhuyin("zhi"), Some("ㄓ".into()));
        assert_eq!(pinyin_to_zhuyin("shi"), Some("ㄕ".into()));
        assert_eq!(pinyin_to_zhuyin("yong"), Some("ㄩㄥ".into()));
        assert_eq!(pinyin_to_zhuyin("wu"), Some("ㄨ".into()));
    }

    #[test]
    fn zhuyin_round_trip() {
        let syllables = all_syllables();
        let mut bad: Vec<String> = Vec::new();
        for syl in &syllables {
            if let Some(code) = encode_with(syl, SCHEME_ZHUYIN) {
                let back = to_full(&code, SCHEME_ZHUYIN);
                if back != *syl {
                    bad.push(format!("{syl} -> {code} -> {back}"));
                }
            } else {
                bad.push(format!("{syl} 无法转注音"));
            }
        }
        assert!(
            bad.is_empty(),
            "注音往返不一致（{} 条）：{:#?}",
            bad.len(),
            bad
        );
    }

    #[test]
    fn zhuyin_typing_flow() {
        // 大千：ㄋ=s，ㄧ=u
        assert_eq!(to_full("su", SCHEME_ZHUYIN), "ni");
        // 你好 = ㄋㄧ ㄏㄠ -> su + cl（ㄏ=c，ㄠ=l）
        assert_eq!(to_full("sucl", SCHEME_ZHUYIN), "nihao");
        // 声调键不影响音节（ˇ = 3）
        assert_eq!(to_full("su3", SCHEME_ZHUYIN), "ni");
        assert_eq!(to_full("su3cl4", SCHEME_ZHUYIN), "nihao");
        // 中国 = ㄓㄨㄥ -> 5 + j + /（ㄓ=5，ㄨ=j，ㄥ=/）
        assert_eq!(to_full("5j/", SCHEME_ZHUYIN), "zhong");
        // 嗯 = ㄥ -> /（零声母）
        assert_eq!(to_full("/", SCHEME_ZHUYIN), "eng");
        // 认不出的字符不进解码
        assert_eq!(to_full("su?", SCHEME_ZHUYIN), "ni");
    }
}
