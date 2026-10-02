//! 快捷输入展开：把一条短前缀输入（算式 / 中文数字 / 金额大写 / Unicode 码点）
//! 展开成可直接上屏的文本，供候选条展示与一键上屏使用。
//!
//! 设计约束：
//! - 只依赖标准库（无 chrono、无第三方 crate），不引用本 crate 其它模块；
//! - 任何非法输入都返回 `None`，绝不 panic、绝不吐垃圾；
//! - 多个前缀同时可匹配时按最长前缀优先（`im` 先于 `i` 判定）。
//!
//! 前缀一览：
//! | 前缀 | 含义 | 示例 | 分类标签 |
//! |---|---|---|---|
//! | `v` | 算式求值 | `v1+2` → `3` | 计算 |
//! | `i` | 中文数字 | `i123` → `一百二十三` | 中文数字 |
//! | `im` | 金额大写 | `im123.45` → `壹佰贰拾叁元肆角伍分` | 金额 |
//! | `u`  | Unicode 码点 | `u4e00` → `一` | 字符 |

/// 快捷输入的一次命中：`text` 是要上屏的内容，`kind` 是给候选条打的分类标签（"计算"/"中文数字"/"金额"/"字符"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub text: String,
    pub kind: &'static str,
}

/// 给定当前输入缓冲（原始串，未规整），若是快捷输入则返回展开结果。
///
/// - 忽略首尾空白；
/// - 不命中 / 非法算不出来一律返回 `None`；
/// - 前缀最长优先：`im123` 走金额，而不是中文数字 `i123`。
pub fn expand(raw: &str) -> Option<Hit> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // 最长前缀优先：先判 `im`，再按首字符分派其余前缀。
    if let Some(body) = s.strip_prefix("im") {
        return amount(body).map(|text| Hit {
            text, kind: "金额"
        });
    }
    let mut it = s.char_indices();
    let first = it.next().map(|(_, c)| c)?;
    let body = &s[first.len_utf8()..];
    match first {
        'v' => calc(body).map(|text| Hit {
            text, kind: "计算"
        }),
        'i' => chinese(body).map(|text| Hit {
            text,
            kind: "中文数字",
        }),
        'u' => codepoint(body).map(|text| Hit {
            text, kind: "字符"
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 1. 算式：v + 算式 -> 结果
// ---------------------------------------------------------------------------

/// 表达式语法树（求值阶段按整数/浮点两种模式分别解释）。
enum Node {
    Num(String),
    Neg(Box<Node>),
    Add(Box<Node>, Box<Node>),
    Sub(Box<Node>, Box<Node>),
    Mul(Box<Node>, Box<Node>),
    Div(Box<Node>, Box<Node>),
    Mod(Box<Node>, Box<Node>),
    Pow(Box<Node>, Box<Node>),
}

/// 整数求值的失败原因。
enum IntErr {
    /// 溢出（含字面量超出 i128）——直接判失败。
    Overflow,
    /// 整数模式表达不了（如负指数），退回浮点模式求值。
    NeedFloat,
    /// 致命错误（除/取模 0、除数异常等）。
    Fatal,
}

/// 递归下降解析器：expr := term (('+'|'-') term)*
struct Parser {
    c: Vec<char>,
    p: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.p).copied()
    }

    fn expr(&mut self) -> Option<Node> {
        let mut n = self.term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.p += 1;
                    let r = self.term()?;
                    n = Node::Add(Box::new(n), Box::new(r));
                }
                Some('-') => {
                    self.p += 1;
                    let r = self.term()?;
                    n = Node::Sub(Box::new(n), Box::new(r));
                }
                _ => break,
            }
        }
        Some(n)
    }

    fn term(&mut self) -> Option<Node> {
        let mut n = self.unary()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.p += 1;
                    let r = self.unary()?;
                    n = Node::Mul(Box::new(n), Box::new(r));
                }
                Some('/') => {
                    self.p += 1;
                    let r = self.unary()?;
                    n = Node::Div(Box::new(n), Box::new(r));
                }
                Some('%') => {
                    self.p += 1;
                    let r = self.unary()?;
                    n = Node::Mod(Box::new(n), Box::new(r));
                }
                _ => break,
            }
        }
        Some(n)
    }

    /// 一元正负号（可叠加），否则进入幂运算。
    fn unary(&mut self) -> Option<Node> {
        match self.peek() {
            Some('+') => {
                self.p += 1;
                self.unary()
            }
            Some('-') => {
                self.p += 1;
                Some(Node::Neg(Box::new(self.unary()?)))
            }
            _ => self.pow(),
        }
    }

    /// 幂运算右结合，指数允许带符号（`2^-1`）。
    fn pow(&mut self) -> Option<Node> {
        let base = self.primary()?;
        if self.peek() == Some('^') {
            self.p += 1;
            let e = self.unary()?;
            Some(Node::Pow(Box::new(base), Box::new(e)))
        } else {
            Some(base)
        }
    }

    /// 主键：数字字面量或括号组。
    fn primary(&mut self) -> Option<Node> {
        match self.peek() {
            Some('(') => {
                self.p += 1;
                let n = self.expr()?;
                if self.peek() == Some(')') {
                    self.p += 1;
                    Some(n)
                } else {
                    None
                }
            }
            Some(d) if d.is_ascii_digit() || d == '.' => {
                let start = self.p;
                let mut dot = false;
                let mut digits = 0usize;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        self.p += 1;
                        digits += 1;
                    } else if ch == '.' && !dot {
                        dot = true;
                        self.p += 1;
                    } else {
                        break;
                    }
                }
                if digits == 0 {
                    // 光秃秃的小数点（`.` 后没数字）——非法。
                    return None;
                }
                let lit: String = self.c[start..self.p].iter().collect();
                Some(Node::Num(lit))
            }
            _ => None,
        }
    }
}

/// 整数模式求值：全整数且只用 `+ - * ^ %`（不含 `/`）时精确计算。
fn eval_int(n: &Node) -> Result<i128, IntErr> {
    match n {
        Node::Num(s) => s.parse::<i128>().map_err(|_| IntErr::Overflow),
        Node::Neg(x) => eval_int(x)?.checked_neg().ok_or(IntErr::Overflow),
        Node::Add(a, b) => {
            let (x, y) = (eval_int(a)?, eval_int(b)?);
            x.checked_add(y).ok_or(IntErr::Overflow)
        }
        Node::Sub(a, b) => {
            let (x, y) = (eval_int(a)?, eval_int(b)?);
            x.checked_sub(y).ok_or(IntErr::Overflow)
        }
        Node::Mul(a, b) => {
            let (x, y) = (eval_int(a)?, eval_int(b)?);
            x.checked_mul(y).ok_or(IntErr::Overflow)
        }
        Node::Div(_, _) => Err(IntErr::Fatal),
        Node::Mod(a, b) => {
            let (x, y) = (eval_int(a)?, eval_int(b)?);
            if y == 0 {
                return Err(IntErr::Fatal);
            }
            x.checked_rem(y).ok_or(IntErr::Overflow)
        }
        Node::Pow(a, b) => {
            let (x, y) = (eval_int(a)?, eval_int(b)?);
            if y < 0 {
                return Err(IntErr::NeedFloat);
            }
            if y > u32::MAX as i128 {
                return Err(IntErr::Overflow);
            }
            x.checked_pow(y as u32).ok_or(IntErr::Overflow)
        }
    }
}

/// 浮点模式求值：任何非有限结果（inf/NaN，如除 0）由调用方判失败。
fn eval_float(n: &Node) -> f64 {
    match n {
        Node::Num(s) => s.parse::<f64>().unwrap_or(f64::NAN),
        Node::Neg(x) => -eval_float(x),
        Node::Add(a, b) => eval_float(a) + eval_float(b),
        Node::Sub(a, b) => eval_float(a) - eval_float(b),
        Node::Mul(a, b) => eval_float(a) * eval_float(b),
        Node::Div(a, b) => eval_float(a) / eval_float(b),
        Node::Mod(a, b) => eval_float(a) % eval_float(b),
        Node::Pow(a, b) => eval_float(a).powf(eval_float(b)),
    }
}

/// 把 f64 格式化为最多 12 位有效数字的十进制串，并去掉尾 0。
///
/// 思路：用 `{:e}` 拿到科学计数法串 `d[.ddd]e±X`，其值等于 `0.数字串 × 10^(X+1)`；
/// 截断/四舍五入到 12 位有效数字后按 `(digits, e10)` 还原成普通十进制。
fn fmt_float(x: f64) -> Option<String> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 {
        return Some("0".to_string());
    }
    let neg = x < 0.0;
    let a = x.abs();
    let sci = format!("{:e}", a);
    let (mant, exp) = sci.split_once('e')?;
    let exp: i32 = exp.trim_start_matches('+').parse().ok()?;
    let mut digits: Vec<u8> = mant.bytes().filter(|b| *b != b'.').collect();
    if digits.is_empty() {
        return None;
    }
    // 值 = 0.digits × 10^e10
    let mut e10 = exp + 1;
    const SIG: usize = 12;
    if digits.len() > SIG {
        let next = digits[SIG];
        let mut keep: Vec<u8> = digits[..SIG].to_vec();
        if next >= b'5' {
            let mut i = SIG;
            let mut carry = true;
            while carry && i > 0 {
                i -= 1;
                if keep[i] == b'9' {
                    keep[i] = b'0';
                } else {
                    keep[i] += 1;
                    carry = false;
                }
            }
            if carry {
                // 进位溢出：999..9 -> 1000..0，值变成原来的 10 倍，指数 +1。
                keep.insert(0, b'1');
                e10 += 1;
            }
        }
        digits = keep;
    }
    // 去尾 0（0.D0 与 0.D 等值）。
    while digits.len() > 1 && *digits.last().unwrap() == b'0' {
        digits.pop();
    }
    let ds: String = digits.iter().map(|b| *b as char).collect();
    let l = ds.len() as i32;
    let mut out = if e10 <= 0 {
        let mut s = String::from("0.");
        for _ in 0..(-e10) {
            s.push('0');
        }
        s.push_str(&ds);
        s
    } else if e10 >= l {
        let mut s = ds.clone();
        for _ in 0..(e10 - l) {
            s.push('0');
        }
        s
    } else {
        let cut = e10 as usize;
        format!("{}.{}", &ds[..cut], &ds[cut..])
    };
    if neg {
        out.insert(0, '-');
    }
    Some(out)
}

/// 解析并计算 `v` 后面的算式。
fn calc(body: &str) -> Option<String> {
    let mut c: Vec<char> = body.trim().chars().collect();
    // 结尾允许可选一个 `=` / 全角 `＝`。
    if let Some(&last) = c.last() {
        if last == '=' || last == '＝' {
            c.pop();
            while c.last().map(|x| x.is_whitespace()).unwrap_or(false) {
                c.pop();
            }
        }
    }
    // 去掉内部空白（算式里的空格一律跳过），同时校验字符集。
    let mut cleaned: Vec<char> = Vec::with_capacity(c.len());
    for ch in c {
        if ch.is_whitespace() {
            continue;
        }
        if ch.is_ascii_digit() || "+-*/^%().".contains(ch) {
            cleaned.push(ch);
        } else {
            return None;
        }
    }
    // `v` 或 `v` 后没出现数字 / `(` -> 不是算式。
    if !cleaned.iter().any(|c| c.is_ascii_digit() || *c == '(') {
        return None;
    }
    // 整数模式：没有 `/` 也没有小数点。
    let float_mode = cleaned.iter().any(|c| *c == '/' || *c == '.');
    let mut parser = Parser { c: cleaned, p: 0 };
    let ast = parser.expr()?;
    if parser.p != parser.c.len() {
        return None; // 残留非法串（如括号不匹配后多出来的字符）
    }
    if float_mode {
        fmt_float(eval_float(&ast))
    } else {
        match eval_int(&ast) {
            Ok(v) => Some(v.to_string()),
            Err(IntErr::NeedFloat) => fmt_float(eval_float(&ast)),
            Err(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// 2. 中文数字：i + 整数/小数 -> 汉字
// ---------------------------------------------------------------------------

/// 支持的绝对值上限：10^16。
const CN_MAX: i128 = 10_000_000_000_000_000;

const CN_DIGITS: [char; 10] = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
/// 节内数位单位，按下标对齐：d[0]=千位, d[1]=百位, d[2]=十位, d[3]=个位。
const CN_SMALL: [&str; 4] = ["千", "百", "十", ""];
/// 四位分节的节单位（第 5 节 10^16 用「亿亿」）。
const CN_SECTION: [&str; 5] = ["", "万", "亿", "万亿", "亿亿"];

/// 把 0..=9999 的一节转成汉字（内部零规范化；`v == 0` 返回空串）。
fn cn_section(v: u32) -> String {
    if v == 0 {
        return String::new();
    }
    let d = [v / 1000 % 10, v / 100 % 10, v / 10 % 10, v % 10];
    let mut s = String::new();
    let mut zero_pending = false;
    for i in 0..4 {
        let digit = d[i];
        if digit == 0 {
            if !s.is_empty() {
                zero_pending = true;
            }
            continue;
        }
        if zero_pending {
            s.push('零');
            zero_pending = false;
        }
        // 「一十」在本节最高位时口语省「一」：10..19 -> 十X。
        if i == 2 && digit == 1 && d[0] == 0 && d[1] == 0 {
            s.push('十');
        } else {
            s.push(CN_DIGITS[digit as usize]);
            s.push_str(CN_SMALL[i]);
        }
    }
    s
}

/// 非负整数转中文数字（四位分节，跨节零规范化）。
fn cn_uint(n: i128) -> String {
    if n == 0 {
        return "零".to_string();
    }
    let mut secs: Vec<u32> = Vec::new();
    let mut rest = n as u128;
    while rest > 0 {
        secs.push((rest % 10_000) as u32);
        rest /= 10_000;
    }
    let mut out = String::new();
    let mut pending_zero = false;
    let mut emitted = false;
    for k in (0..secs.len()).rev() {
        let v = secs[k];
        if v == 0 {
            if emitted {
                pending_zero = true;
            }
            continue;
        }
        // 需要补「零」的两种情形：中间整节为 0（跳节），或本节千位为 0（与上一节之间断档）。
        if pending_zero || (emitted && v < 1000) {
            out.push('零');
            pending_zero = false;
        }
        out.push_str(&cn_section(v));
        out.push_str(CN_SECTION[k]);
        emitted = true;
    }
    out
}

/// 解析 `i` 后面的数字（可带符号、可带小数），转成中文数字。
///
/// 不完整状态（`i`、`i-`、`i.`、`i5.`）以及夹杂字母的输入返回 `None`。
fn chinese(body: &str) -> Option<String> {
    let (neg, rest) = match body.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, body),
    };
    let (ip, fp) = match rest.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    // `i` 后跟字母（如 `im` 之外的其它字母）不走中文数字。
    if !ip.is_empty() && !ip.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some(f) = fp {
        if f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    if ip.is_empty() && fp.is_none() {
        return None; // 光秃秃一个 `i` / `i-`
    }
    let n: i128 = if ip.is_empty() {
        0
    } else {
        ip.parse::<i128>().ok()?
    };
    if n > CN_MAX {
        return None;
    }
    let mut out = String::new();
    if neg && (n != 0 || fp.is_some()) {
        out.push('负');
    }
    out.push_str(&cn_uint(n));
    if let Some(f) = fp {
        out.push('点');
        for ch in f.chars() {
            out.push(CN_DIGITS[(ch as u8 - b'0') as usize]);
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// 3. 金额大写：im + 数字 -> 中文大写金额
// ---------------------------------------------------------------------------

/// 金额整数部分上限：10^10（100 亿）。
const AMT_MAX: i128 = 10_000_000_000;

const AMT_DIGITS: [char; 10] = ['零', '壹', '贰', '叁', '肆', '伍', '陆', '柒', '捌', '玖'];
/// 节内数位单位，按下标对齐：d[0]=仟位, d[1]=佰位, d[2]=拾位, d[3]=个位。
const AMT_SMALL: [&str; 4] = ["仟", "佰", "拾", ""];
const AMT_SECTION: [&str; 3] = ["", "万", "亿"];

/// 把 0..=9999 的一节转成大写（`v == 0` 返回空串）。
fn amt_section(v: u32) -> String {
    if v == 0 {
        return String::new();
    }
    let d = [v / 1000 % 10, v / 100 % 10, v / 10 % 10, v % 10];
    let mut s = String::new();
    let mut zero_pending = false;
    for i in 0..4 {
        let digit = d[i];
        if digit == 0 {
            if !s.is_empty() {
                zero_pending = true;
            }
            continue;
        }
        if zero_pending {
            s.push('零');
            zero_pending = false;
        }
        s.push(AMT_DIGITS[digit as usize]);
        s.push_str(AMT_SMALL[i]);
    }
    s
}

/// 金额整数部分（元）转大写：0 -> 「零」，四位分节带 万/亿。
fn amt_uint(n: i128) -> String {
    if n == 0 {
        return "零".to_string();
    }
    let mut secs: Vec<u32> = Vec::new();
    let mut rest = n as u128;
    while rest > 0 {
        secs.push((rest % 10_000) as u32);
        rest /= 10_000;
    }
    let mut out = String::new();
    let mut pending_zero = false;
    let mut emitted = false;
    for k in (0..secs.len()).rev() {
        let v = secs[k];
        if v == 0 {
            if emitted {
                pending_zero = true;
            }
            continue;
        }
        // 与中文数字同规则：跳节或本节仟位为 0 时补「零」。
        if pending_zero || (emitted && v < 1000) {
            out.push('零');
            pending_zero = false;
        }
        out.push_str(&amt_section(v));
        out.push_str(AMT_SECTION[k]);
        emitted = true;
    }
    out
}

/// 解析 `im` 后面的数字，转成中文大写金额。
///
/// 整数到「元」且没有角分 -> 加「整」；有角分则以角/分结尾不加「整」；
/// 角位为 0 而分位非 0 -> 写「零」；整数部分 ≥ 10^10 -> `None`。
fn amount(body: &str) -> Option<String> {
    let (ip, fp) = match body.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (body, None),
    };
    if ip.is_empty() && fp.is_none() {
        return None; // 光秃秃一个 `im`
    }
    if !ip.is_empty() && !ip.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if let Some(f) = fp {
        if f.is_empty() || f.len() > 2 || !f.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
    }
    let n: i128 = if ip.is_empty() {
        0
    } else {
        ip.parse::<i128>().ok()?
    };
    if n >= AMT_MAX {
        return None;
    }
    let mut out = amt_uint(n);
    out.push('元');
    let frac = match fp {
        Some(f) => {
            let mut b = f.as_bytes().to_vec();
            while b.len() < 2 {
                b.push(b'0');
            }
            Some((b[0] - b'0', b[1] - b'0'))
        }
        None => None,
    };
    match frac {
        None => out.push('整'),
        Some((jiao, fen)) => {
            if jiao == 0 && fen == 0 {
                out.push('整');
            } else if jiao != 0 {
                out.push(AMT_DIGITS[jiao as usize]);
                out.push('角');
                if fen != 0 {
                    out.push(AMT_DIGITS[fen as usize]);
                    out.push('分');
                }
            } else {
                // 角位 0 而分位非 0 -> 「零」+ 分。
                out.push('零');
                out.push(AMT_DIGITS[fen as usize]);
                out.push('分');
            }
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// 4. Unicode 码点：u + 十六进制 -> 字符
// ---------------------------------------------------------------------------

/// 解析 `u` 后面的码点：允许可选 `0x`/`0X` 前缀，1..6 位，大小写都认。
///
/// 进制判定：去掉可选前缀后，含 `a-f` 字母按十六进制（`u4e00` -> `一`、`u4e` -> `P`）；
/// 纯十进制数字按十进制（`u65` -> `A`，与 `u0x65` -> `A` 一致）。
/// 非法码点（超出 Unicode 范围、代理区 D800..DFFF）返回 `None`。
fn codepoint(body: &str) -> Option<String> {
    let hex = body
        .strip_prefix("0x")
        .or_else(|| body.strip_prefix("0X"))
        .unwrap_or(body);
    if hex.is_empty() || hex.len() > 6 {
        return None;
    }
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = if hex.bytes().any(|b| b.is_ascii_alphabetic()) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        hex.parse::<u32>().ok()?
    };
    // `char::from_u32` 一并拒绝超范围与 D800..DFFF 代理区。
    let ch = char::from_u32(v)?;
    Some(ch.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> Option<String> {
        expand(raw).map(|h| h.text)
    }

    fn kind(raw: &str) -> Option<&'static str> {
        expand(raw).map(|h| h.kind)
    }

    // ---- 算式 ----------------------------------------------------------

    #[test]
    fn calc_basic_examples() {
        assert_eq!(text("v1+2").as_deref(), Some("3"));
        assert_eq!(text("v(2+3)*4").as_deref(), Some("20"));
        assert_eq!(text("v10/4").as_deref(), Some("2.5"));
        assert_eq!(text("v2^10").as_deref(), Some("1024"));
        assert_eq!(text("v100*1.08").as_deref(), Some("108"));
        assert_eq!(text("v18%5").as_deref(), Some("3"));
        assert_eq!(text("v2-5").as_deref(), Some("-3"));
        assert_eq!(kind("v1+2"), Some("计算"));
    }

    #[test]
    fn calc_precision_and_syntax() {
        assert_eq!(text("v0.1+0.2").as_deref(), Some("0.3"));
        assert_eq!(text("v1/3").as_deref(), Some("0.333333333333"));
        assert_eq!(text("v100*1.08").as_deref(), Some("108"));
        assert_eq!(text("v1+2=").as_deref(), Some("3"));
        assert_eq!(text("v1+2＝").as_deref(), Some("3"));
        assert_eq!(text("v 1 + 2 ").as_deref(), Some("3"));
        assert_eq!(text("v2/4").as_deref(), Some("0.5"));
        assert_eq!(text("v10%3").as_deref(), Some("1"));
        assert_eq!(text("v2^0").as_deref(), Some("1"));
    }

    #[test]
    fn calc_invalid_returns_none() {
        assert_eq!(text("v"), None);
        assert_eq!(text("v="), None);
        assert_eq!(text("v+"), None);
        assert_eq!(text("v/2"), None);
        assert_eq!(text("vabc"), None);
        assert_eq!(text("v1/0"), None);
        assert_eq!(text("v0/0"), None);
        assert_eq!(text("v(1+2"), None);
        assert_eq!(text("v1+2)"), None);
        assert_eq!(text("v1..2"), None);
        assert_eq!(text("v1&2"), None);
        assert_eq!(text("v9^999999999"), None); // 溢出
    }

    // ---- 中文数字 ------------------------------------------------------

    #[test]
    fn chinese_basic_examples() {
        assert_eq!(text("i0").as_deref(), Some("零"));
        assert_eq!(text("i5").as_deref(), Some("五"));
        assert_eq!(text("i10").as_deref(), Some("十"));
        assert_eq!(text("i15").as_deref(), Some("十五"));
        assert_eq!(text("i20").as_deref(), Some("二十"));
        assert_eq!(text("i123").as_deref(), Some("一百二十三"));
        assert_eq!(text("i1005").as_deref(), Some("一千零五"));
        assert_eq!(text("i10000").as_deref(), Some("一万"));
        assert_eq!(text("i100000000").as_deref(), Some("一亿"));
        assert_eq!(text("i10000000").as_deref(), Some("一千万"));
        assert_eq!(
            text("i123456789").as_deref(),
            Some("一亿二千三百四十五万六千七百八十九")
        );
        assert_eq!(text("i100001000").as_deref(), Some("一亿零一千"));
        assert_eq!(text("i12.34").as_deref(), Some("十二点三四"));
        assert_eq!(text("i-5").as_deref(), Some("负五"));
        assert_eq!(kind("i15"), Some("中文数字"));
    }

    #[test]
    fn chinese_zero_rules() {
        assert_eq!(text("i1010").as_deref(), Some("一千零一十"));
        assert_eq!(text("i10000005").as_deref(), Some("一千万零五"));
        assert_eq!(text("i110").as_deref(), Some("一百一十"));
        assert_eq!(text("i100000000000").as_deref(), Some("一千亿"));
        assert_eq!(text("i1000000000000").as_deref(), Some("一万亿"));
        // 相邻两节、但本节千位为 0 -> 补一个零。
        assert_eq!(text("i100000010000").as_deref(), Some("一千亿零一万"));
        // 中间整节为 0 -> 补一个零。
        assert_eq!(text("i100000001000").as_deref(), Some("一千亿零一千"));
        assert_eq!(text("i10000000000000000").as_deref(), Some("一亿亿")); // 10^16 上限
    }

    #[test]
    fn chinese_invalid_returns_none() {
        assert_eq!(text("i"), None);
        assert_eq!(text("i-"), None);
        assert_eq!(text("i."), None);
        assert_eq!(text("i5."), None);
        assert_eq!(text("ix"), None);
        assert_eq!(text("i5a"), None);
        assert_eq!(text("i10000000000000001"), None); // 超出 10^16
        assert_eq!(text("i99999999999999999999"), None);
    }

    // ---- 金额大写 ------------------------------------------------------

    #[test]
    fn amount_basic_examples() {
        assert_eq!(text("im123.45").as_deref(), Some("壹佰贰拾叁元肆角伍分"));
        assert_eq!(text("im100").as_deref(), Some("壹佰元整"));
        assert_eq!(text("im0.5").as_deref(), Some("零元伍角"));
        assert_eq!(text("im0.05").as_deref(), Some("零元零伍分"));
        assert_eq!(text("im20000000").as_deref(), Some("贰仟万元整"));
        assert_eq!(text("im1005.06").as_deref(), Some("壹仟零伍元零陆分"));
        assert_eq!(text("im0").as_deref(), Some("零元整"));
        // 跨节断档补零（与中文数字同规则）。
        assert_eq!(text("im10000005").as_deref(), Some("壹仟万零伍元整"));
        assert_eq!(kind("im100"), Some("金额"));
    }

    #[test]
    fn amount_invalid_returns_none() {
        assert_eq!(text("im"), None);
        assert_eq!(text("im."), None);
        assert_eq!(text("im5."), None);
        assert_eq!(text("imabc"), None);
        assert_eq!(text("im12.345"), None); // 超过两位小数
        assert_eq!(text("im10000000000"), None); // ≥10^10
        assert_eq!(text("im-3"), None);
    }

    // ---- Unicode 码点 --------------------------------------------------

    #[test]
    fn codepoint_examples() {
        assert_eq!(text("u4e00").as_deref(), Some("一"));
        assert_eq!(text("u65").as_deref(), Some("A"));
        assert_eq!(text("u0x65").as_deref(), Some("A"));
        assert_eq!(text("u0X65").as_deref(), Some("A"));
        assert_eq!(text("u4e").as_deref(), Some("N")); // 0x4E = 'N'
        assert_eq!(text("u1F600").as_deref(), Some("😀"));
        assert_eq!(text("u4E00").as_deref(), Some("一"));
        // 纯十进制数字按十进制解析（与 `u65` -> `A` 同规则）。
        assert_eq!(text("u20013").as_deref(), Some("中"));
        assert_eq!(kind("u4e00"), Some("字符"));
    }

    #[test]
    fn codepoint_invalid_returns_none() {
        assert_eq!(text("u"), None);
        assert_eq!(text("u0x"), None);
        assert_eq!(text("uzzzz"), None);
        assert_eq!(text("u4G00"), None);
        assert_eq!(text("u55296"), None); // 55296 = 0xD800，代理区
        assert_eq!(text("uD800"), None); // 代理区
        assert_eq!(text("udfff"), None); // 代理区
        assert_eq!(text("uFFFFFF"), None); // 16777215 > 0x10FFFF
        assert_eq!(text("u1114112"), None); // 超长且超出范围
    }

    // ---- 前缀最长优先 / 前言 ------------------------------------------------

    #[test]
    fn prefix_longest_wins() {
        // `im123` 必须走金额，而不是中文数字 `i123`。
        assert_eq!(text("im123").as_deref(), Some("壹佰贰拾叁元整"));
        assert_eq!(kind("im123"), Some("金额"));
        // 没有 `im` 时 `i123` 才是中文数字。
        assert_eq!(text("i123").as_deref(), Some("一百二十三"));
        assert_eq!(kind("i123"), Some("中文数字"));
    }

    #[test]
    fn trim_and_unrelated_input() {
        assert_eq!(text("  v1+2  ").as_deref(), Some("3"));
        assert_eq!(text(" i5 ").as_deref(), Some("五"));
        assert_eq!(text("  "), None);
        assert_eq!(text(""), None);
        assert_eq!(text("hello"), None);
        assert_eq!(text("123"), None);
        assert_eq!(text("="), None);
    }

    #[test]
    fn hit_derives_and_clone() {
        let h = expand("v1+2").expect("should hit");
        let c = h.clone();
        assert_eq!(h, c);
        assert_eq!(h.text, "3");
        assert_eq!(h.kind, "计算");
        // Debug 输出包含结构名与字段。
        let dbg = format!("{:?}", h);
        assert!(dbg.contains("Hit"));
        assert!(dbg.contains("kind"));
    }
}
