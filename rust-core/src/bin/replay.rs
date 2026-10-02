//! 回放评测：拿一批「击键 -> 期望词」用例跑过引擎，报告 top-1 / top-5 命中率。
//!
//! 每次改完引擎或键位表，一条命令就能看出质量有没有被改坏：
//!
//! ```text
//! cargo run --bin replay -- --limit 50 --scheme 1
//! cargo run --bin replay -- --tsv cases.tsv --scheme 4 --verbose
//! ```
//!
//! 用例两条来源（都支持）：
//! - 不带 `--tsv`：从引擎内置词典采样「拼音 -> 词」自举出用例，不需要任何数据目录；
//! - 带 `--tsv`：外部用例文件，每行 `期望词<TAB>击键`（可选第 3 列方案编号）。
//!
//! 评测先把击键按 `--scheme` 还原成全拼，再走 [`engine::analyze`] 拿前 5 个候选，
//! 看期望词排在第几。全局选项在进程内固定（模糊音/纠错开、方案 0），避免互相干扰。

use typesake_core::{engine, shuangpin};

/// 方案编号上界（0=全拼 … 8=大千注音）。
const SCHEME_MAX: u8 = 8;

/// 默认评测条数。
const DEFAULT_LIMIT: usize = 500;

/// 看前多少名才算 top-N（同时决定给引擎要多少候选）。
const TOP_N: usize = 5;

/// 失败样例最多记录几条。
const MAX_SAMPLES: usize = 20;

const USAGE: &str = "\
回放评测：用「击键 -> 期望词」用例跑引擎，报告 top-1 / top-5 命中率。

用法：
  replay [选项]

选项：
  --tsv <path>    外部用例文件，每行 `期望词<TAB>全拼击键`，可选第 3 列方案编号；
                  `#` 开头的行与空行跳过，文件必须是 UTF-8。
                  不提供时从引擎内置词典自动采样用例（不需要数据目录）。
  --scheme <0-8>  方案编号，默认 0（全拼）：
                  0=全拼 1=小鹤 2=自然码 3=微软 4=搜狗 5=智能ABC 6=小浪 7=首道 8=大千注音
  --limit <n>     最多评测多少条，默认 500
  --verbose       打印失败样例（最多 20 条）
  --json          输出一行 JSON（方案、总数、top1、top5、覆盖率、失败样例前 20）
  --help          显示本帮助

退出码：0 = 正常跑完；1 = 参数错误 / 用例为空 / 数据加载失败。

示例：
  cargo run --bin replay -- --limit 50 --scheme 1
  cargo run --bin replay -- --tsv cases.tsv --scheme 4 --verbose";

// ---------------- 参数解析 ----------------

/// 命令行配置（`parse_args` 的输出，`main` 只做编排）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cfg {
    /// 外部用例文件；`None` 表示走内置词典自举
    tsv: Option<String>,
    scheme: u8,
    limit: usize,
    verbose: bool,
    json: bool,
    help: bool,
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            tsv: None,
            scheme: 0,
            limit: DEFAULT_LIMIT,
            verbose: false,
            json: false,
            help: false,
        }
    }
}

/// 解析命令行；未知参数、缺取值、越界取值一律返回 `Err`（调用方打印用法并退出 1）。
fn parse_args(args: &[String]) -> Result<Cfg, String> {
    let mut cfg = Cfg::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => {
                cfg.help = true;
                return Ok(cfg);
            }
            "--verbose" => cfg.verbose = true,
            "--json" => cfg.json = true,
            "--tsv" => {
                let v = it
                    .next()
                    .ok_or_else(|| "缺少取值：--tsv <path>".to_string())?;
                cfg.tsv = Some(v.clone());
            }
            "--scheme" => {
                let v = it
                    .next()
                    .ok_or_else(|| "缺少取值：--scheme <0-8>".to_string())?;
                let n: u8 = v
                    .parse()
                    .map_err(|_| format!("方案编号必须是 0-{SCHEME_MAX} 的整数：{v}"))?;
                if n > SCHEME_MAX {
                    return Err(format!("方案编号超出范围 0-{SCHEME_MAX}：{n}"));
                }
                cfg.scheme = n;
            }
            "--limit" => {
                let v = it
                    .next()
                    .ok_or_else(|| "缺少取值：--limit <n>".to_string())?;
                let n: usize = v.parse().map_err(|_| format!("条数必须是正整数：{v}"))?;
                if n == 0 {
                    return Err("条数必须大于 0".to_string());
                }
                cfg.limit = n;
            }
            other => return Err(format!("未知参数：{other}")),
        }
    }
    Ok(cfg)
}

// ---------------- 用例 ----------------

/// 一条用例：期望词 + 击键（按方案解释）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Case {
    /// 期望出现在候选里的词
    word: String,
    /// 用户敲的键（按 `scheme` 解读）
    keys: String,
    /// 单条覆盖的方案；`None` 表示用全局 `--scheme`
    scheme: Option<u8>,
}

/// 解析 TSV 用例：`期望词<TAB>击键` 或 `期望词<TAB>击键<TAB>方案编号`。
/// `#` 开头的行与空行跳过，字段两端空白忽略，不合法的行直接跳过。
fn parse_tsv(text: &str) -> Vec<Case> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        if fields.len() < 2 || fields.len() > 3 {
            continue;
        }
        let (word, keys) = (fields[0], fields[1]);
        if word.is_empty() || keys.is_empty() {
            continue;
        }
        let scheme = match fields.get(2) {
            None => None,
            Some(s) => match s.parse::<u8>() {
                Ok(n) if n <= SCHEME_MAX => Some(n),
                _ => continue,
            },
        };
        out.push(Case {
            word: word.to_string(),
            keys: keys.to_string(),
            scheme,
        });
    }
    out
}

/// xorshift64*：确定性伪随机，保证同一条命令每次采样出同一批用例。
fn next_rand(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// 全拼串 -> 指定方案的击键串（逐音节编码）。
/// 任何音节编不出来就返回 `None`（该条目不参与评测）。
fn encode_pinyin(pinyin: &str, scheme: u8) -> Option<String> {
    if pinyin.is_empty() {
        return None;
    }
    if scheme == 0 {
        // 全拼：原样即击键
        return Some(pinyin.to_string());
    }
    let seg = inputx_pinyin::segment(pinyin).into_iter().next()?;
    let mut out = String::new();
    for syl in &seg.syllables {
        out.push_str(&shuangpin::encode_with(syl, scheme)?);
    }
    (!out.is_empty()).then_some(out)
}

/// 内置自举：从引擎自己的词典流式采样，凑出 `limit` 条用例。
///
/// 采样是**等间隔跨全书**再洗牌，而不是只扫词典前缀——词典按拼音排序，
/// 只取前缀会退化成「全是 a 开头的词」，别的声母一条都测不到。
/// 编码（全拼 -> 击键）比遍历贵两三个数量级，所以按 stride 稀疏取样，
/// 全书只编码几百条。
///
/// 只收「当前方案能编码成击键」的条目（编码不出来就没法评）；不做过滤式挑选，
/// 键位表如果改坏了往返，会直接体现成命中率下降——这正是本工具要抓的信号。
fn bootstrap_cases(limit: usize, scheme: u8) -> Result<Vec<Case>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    // 只问词典规模：lexicon_info() 会连带触发简拼索引的惰性建表
    // （对全词典逐条 segment），debug 下极慢，评测用不到
    let lex = engine::lexicon_size();
    if lex == 0 {
        return Err(format!("内置词典加载失败（词条数 {lex}），无法自举用例"));
    }
    let dict = engine::engine().dict();
    // 目标候选量留 4 倍余量抵消编码失败；上限防止 --limit 很大时把全书扫得太密
    let target = limit.saturating_mul(4).clamp(64, 20_000);
    let stride = ((lex / target).max(1)) as u64;
    let mut pool: Vec<Case> = Vec::new();
    let mut idx = 0u64;
    dict.prefix_for_each("", |pinyin, word, _freq| {
        let here = idx;
        idx += 1;
        if !here.is_multiple_of(stride) {
            return;
        }
        if let Some(keys) = encode_pinyin(pinyin, scheme) {
            pool.push(Case {
                word: word.to_string(),
                keys,
                scheme: None,
            });
        }
    });
    if pool.is_empty() {
        return Err("内置词典里没有可编码的条目，无法自举用例".to_string());
    }
    // 洗牌后截断：固定随机源，同样的参数跑出来的用例完全一致（方便对比改动前后）
    let mut rng = 0x1234_5678_9ABC_DEF0u64;
    for i in (1..pool.len()).rev() {
        let j = (next_rand(&mut rng) % (i as u64 + 1)) as usize;
        pool.swap(i, j);
    }
    pool.truncate(limit);
    Ok(pool)
}

/// 载入用例：外部 TSV 优先，否则自举。失败返回 `Err`（调用方退出 1）。
fn load_cases(cfg: &Cfg) -> Result<Vec<Case>, String> {
    match &cfg.tsv {
        Some(path) => {
            let bytes =
                std::fs::read(path).map_err(|e| format!("读取用例文件 {path} 失败：{e}"))?;
            let text =
                String::from_utf8(bytes).map_err(|_| format!("用例文件 {path} 不是合法 UTF-8"))?;
            let mut cases = parse_tsv(&text);
            cases.truncate(cfg.limit);
            Ok(cases)
        }
        None => bootstrap_cases(cfg.limit, cfg.scheme),
    }
}

// ---------------- 评测 ----------------

/// 一条失败样例（供人类可读与 JSON 输出）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sample {
    word: String,
    keys: String,
    /// 击键还原出来的全拼
    full: String,
    /// 实际候选的前 3 个
    top3: Vec<String>,
}

/// 一批用例的评测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Report {
    scheme: u8,
    total: usize,
    top1: usize,
    top5: usize,
    /// 有候选的条数（覆盖 = 有候选 / 总数）
    covered: usize,
    /// 失败样例，最多 [`MAX_SAMPLES`] 条
    failures: Vec<Sample>,
}

/// 逐条跑引擎：击键按方案还原成全拼 -> 拿前 5 个候选 -> 判命中。
fn evaluate(cases: &[Case], scheme: u8) -> Report {
    let mut r = Report {
        scheme,
        total: cases.len(),
        top1: 0,
        top5: 0,
        covered: 0,
        failures: Vec::new(),
    };
    for c in cases {
        let s = c.scheme.unwrap_or(scheme);
        let full = shuangpin::to_full(&c.keys, s);
        let top: Vec<String> = engine::analyze(&full, TOP_N)
            .candidates
            .into_iter()
            .take(TOP_N)
            .collect();
        if !top.is_empty() {
            r.covered += 1;
        }
        let hit1 = top.first().is_some_and(|w| w == &c.word);
        let hit5 = top.iter().any(|w| w == &c.word);
        if hit1 {
            r.top1 += 1;
        }
        if hit5 {
            r.top5 += 1;
        } else if r.failures.len() < MAX_SAMPLES {
            r.failures.push(Sample {
                word: c.word.clone(),
                keys: c.keys.clone(),
                full,
                top3: top.iter().take(3).cloned().collect(),
            });
        }
    }
    r
}

/// 百分比：保留 1 位小数，分母为 0 打 `n/a`（不打 NaN）。
fn fmt_pct(num: usize, den: usize) -> String {
    match pct_value(num, den) {
        Some(v) => format!("{v:.1}%"),
        None => "n/a".to_string(),
    }
}

/// 百分比数值（保留 1 位小数）；分母为 0 返回 `None`（JSON 里写 null）。
fn pct_value(num: usize, den: usize) -> Option<f64> {
    if den == 0 {
        return None;
    }
    Some((num as f64 * 1000.0 / den as f64).round() / 10.0)
}

// ---------------- 输出 ----------------

/// 人类可读报告。
fn print_human(r: &Report, verbose: bool) {
    println!(
        "方案：{}   用例：{}   top-1: {}   top-5: {}   覆盖(有候选): {}",
        shuangpin::scheme_name(r.scheme),
        r.total,
        fmt_pct(r.top1, r.total),
        fmt_pct(r.top5, r.total),
        fmt_pct(r.covered, r.total),
    );
    let failed = r.total - r.top5;
    if !verbose {
        if failed > 0 {
            println!("失败：{failed} 条（--verbose 查看样例）");
        }
        return;
    }
    if r.failures.is_empty() {
        println!("失败样例：无");
        return;
    }
    println!("失败样例（前 {}）：", r.failures.len());
    for s in &r.failures {
        println!(
            "  期望「{}」 击键「{}」 还原「{}」 top-3: [{}]",
            s.word,
            s.keys,
            s.full,
            s.top3.join(", ")
        );
    }
    if failed > r.failures.len() {
        println!("（另有 {} 条失败未展示）", failed - r.failures.len());
    }
}

/// 机器可读报告：一行 JSON，失败样例固定取前 [`MAX_SAMPLES`] 条。
fn report_json(r: &Report) -> String {
    let failures: Vec<serde_json::Value> = r
        .failures
        .iter()
        .map(|s| {
            serde_json::json!({
                "expect": s.word,
                "keys": s.keys,
                "full": s.full,
                "top3": s.top3,
            })
        })
        .collect();
    serde_json::json!({
        "scheme": shuangpin::scheme_name(r.scheme),
        "scheme_id": r.scheme,
        "total": r.total,
        "top1": pct_value(r.top1, r.total),
        "top5": pct_value(r.top5, r.total),
        "coverage": pct_value(r.covered, r.total),
        "failures": failures,
    })
    .to_string()
}

// ---------------- 入口 ----------------

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    };
    if cfg.help {
        println!("{USAGE}");
        return;
    }
    let cases = match load_cases(&cfg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("错误：{e}");
            std::process::exit(1);
        }
    };
    if cases.is_empty() {
        eprintln!("错误：用例为空（检查 --tsv 文件内容或 --limit）");
        std::process::exit(1);
    }
    // 评测只看候选质量：全局选项固定，避免宿主设置污染结果
    engine::set_options(true, true, 0);
    let report = evaluate(&cases, cfg.scheme);
    if cfg.json {
        println!("{}", report_json(&report));
    } else {
        print_human(&report, cfg.verbose);
    }
}

// ---------------- 测试 ----------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // ---- 参数解析 ----

    #[test]
    fn args_defaults_and_help() {
        let c = parse_args(&[]).unwrap();
        assert_eq!(c, Cfg::default());
        assert_eq!(c.scheme, 0);
        assert_eq!(c.limit, DEFAULT_LIMIT);

        let h = parse_args(&args(&["--help"])).unwrap();
        assert!(h.help);
        let h2 = parse_args(&args(&["-h"])).unwrap();
        assert!(h2.help);
    }

    #[test]
    fn args_unknown_flag_is_error() {
        assert!(parse_args(&args(&["--nope"])).is_err());
        assert!(parse_args(&args(&["junk"])).is_err());
        // 未知参数必须是非 0 退出码才能当参数错误
        assert!(parse_args(&args(&["--verbose", "--bogus"])).is_err());
    }

    #[test]
    fn args_missing_value_is_error() {
        assert!(parse_args(&args(&["--tsv"])).is_err());
        assert!(parse_args(&args(&["--scheme"])).is_err());
        assert!(parse_args(&args(&["--limit"])).is_err());
    }

    #[test]
    fn args_scheme_range_checked() {
        let c = parse_args(&args(&["--scheme", "4"])).unwrap();
        assert_eq!(c.scheme, 4);
        let c = parse_args(&args(&["--scheme", "8"])).unwrap();
        assert_eq!(c.scheme, 8);
        assert!(parse_args(&args(&["--scheme", "9"])).is_err());
        assert!(parse_args(&args(&["--scheme", "abc"])).is_err());
    }

    #[test]
    fn args_limit_checked() {
        let c = parse_args(&args(&["--limit", "10"])).unwrap();
        assert_eq!(c.limit, 10);
        assert!(parse_args(&args(&["--limit", "0"])).is_err());
        assert!(parse_args(&args(&["--limit", "abc"])).is_err());
    }

    #[test]
    fn args_flags_and_tsv_path() {
        let c = parse_args(&args(&["--tsv", "cases.tsv", "--verbose", "--json"])).unwrap();
        assert_eq!(c.tsv.as_deref(), Some("cases.tsv"));
        assert!(c.verbose);
        assert!(c.json);
        assert!(!c.help);
    }

    // ---- TSV 解析 ----

    #[test]
    fn tsv_skips_comments_and_blank_lines() {
        let text = "# 期望词\t击键\n\n   \n明天\tmyvt\n";
        let cases = parse_tsv(text);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].word, "明天");
        assert_eq!(cases[0].keys, "myvt");
        assert_eq!(cases[0].scheme, None);
    }

    #[test]
    fn tsv_skips_malformed_lines() {
        let text = concat!(
            "没有制表符\n",       // 字段不足
            "\tnt\n",             // 期望词为空
            "明天\t\n",           // 击键为空
            "今天\tjintian\t9\n", // 方案编号越界
            "后天\thoutian\tx\n", // 方案编号非数字
            "明天\tmyvt\n",       // 合法
            "中国\tvsgo\t1\n",    // 合法（带方案列）
        );
        let cases = parse_tsv(text);
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].word, "明天");
        assert_eq!(cases[1].scheme, Some(1));
        assert_eq!(cases[1].keys, "vsgo");
    }

    #[test]
    fn tsv_crlf_and_spaces_are_tolerated() {
        let cases = parse_tsv("  明天 \t myvt \r\n中国\tvsgo\r\n");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].word, "明天");
        assert_eq!(cases[0].keys, "myvt");
        assert_eq!(cases[1].keys, "vsgo");
    }

    // ---- 百分比 ----

    #[test]
    fn pct_format_one_decimal() {
        assert_eq!(fmt_pct(1, 3), "33.3%");
        assert_eq!(fmt_pct(5, 5), "100.0%");
        assert_eq!(fmt_pct(0, 4), "0.0%");
        assert_eq!(fmt_pct(462, 500), "92.4%");
    }

    #[test]
    fn pct_zero_denominator_is_na() {
        assert_eq!(fmt_pct(0, 0), "n/a");
        assert!(pct_value(0, 0).is_none());
        assert_eq!(pct_value(1, 3), Some(33.3));
        assert_eq!(pct_value(485, 500), Some(97.0));
    }

    // ---- 自举编码（纯逻辑，不碰引擎全局状态） ----

    #[test]
    fn encode_pinyin_scheme_zero_is_passthrough() {
        assert_eq!(encode_pinyin("zhongguo", 0).as_deref(), Some("zhongguo"));
        assert_eq!(encode_pinyin("", 0), None);
    }

    #[test]
    fn encode_pinyin_round_trips_through_to_full() {
        for scheme in 0..=SCHEME_MAX {
            let keys = encode_pinyin("nihao", scheme).unwrap();
            assert_eq!(
                shuangpin::to_full(&keys, scheme),
                "nihao",
                "方案 {scheme} 往返失败：{keys}"
            );
        }
    }

    // ---- 报告输出 ----

    #[test]
    fn json_report_shape() {
        let r = Report {
            scheme: 1,
            total: 2,
            top1: 1,
            top5: 2,
            covered: 2,
            failures: Vec::new(),
        };
        let s = report_json(&r);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["scheme"], "小鹤双拼");
        assert_eq!(v["scheme_id"], 1);
        assert_eq!(v["total"], 2);
        assert_eq!(v["top1"], 50.0);
        assert_eq!(v["top5"], 100.0);
        assert!(v["failures"].as_array().is_some_and(|a| a.is_empty()));
    }
}
