//! JNI 边界。
//!
//! 约定：
//! - 热路径（每键调用）返回分隔串而非 JSON：
//!   `analyzeInput` = `flag<US>matched<US>c1<GS>c2<GS>…`
//!   （US='\u{1E}' 字段分隔，GS='\u{1F}' 列表分隔，省掉每键 JSON 解析）
//! - 冷路径（收藏/统计/初始化）返回 JSON；
//! - **所有导出函数都经 [`guarded`] 包裹**：Rust panic 不跨 FFI，最坏返回空串。
//! - 入参转换 [`jstr_to_rust`] 与出参转换 [`rust_to_jstr`] 自身永不 panic
//!  （null 入参/pending exception/OOM 全降级为空串或 null JString），
//!   因此在 `guarded` 之外调用也是安全的。

use crate::{engine, english, store};
use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint};
use jni::JNIEnv;

pub const FIELD: char = '\u{1E}';
/// 字段分隔（US），与 [`crate::DELIM`]（GS='\u{1F}' 列表分隔）配对。
/// 跨语言协议见 `docs/proto.md`：US=字段，GS=列表，RS='\u{1D}'=子字段。
pub const UNIT_SEP: char = '\u{1E}';
/// 列表分隔（GS），与 [`crate::DELIM_STR`] 同值。此处保留 `ITEM` 别名仅为兼容旧名，
/// 新代码请用 `crate::DELIM` / `crate::DELIM_STR`。
pub const ITEM: char = '\u{1F}';
/// 子字段分隔（RS）：`拼音<RS>词`、`kind<RS>text` 等都用它。
pub const REC_SEP: char = '\u{1D}';

/// JNI 入参 -> Rust String（永不 panic，供 FFI 边界外调用）。
///
/// - null 入参 -> 空串；
/// - JVM 已有 pending exception -> 空串（不清异常，让调用方决定）；
/// - `get_string` / `to_str` 失败 -> 空串（不 `unwrap` / `expect`）。
fn jstr_to_rust(env: &mut JNIEnv, s: &JString) -> String {
    use jni::objects::JObject;
    if s.is_null() {
        return String::new();
    }
    // JString derefs to JObject；is_null 上面已查。这里再防一次 pending exception。
    if env.exception_check().unwrap_or(true) {
        return String::new();
    }
    env.get_string(s)
        .map(|j| j.to_str().unwrap_or_default().to_owned())
        .unwrap_or_default()
}

/// Rust String -> JNI String（永不 panic）。
///
/// `new_string` 失败（OOM / pending exception）时退回空串；
/// 空串也建不出来时返回 null JString（Kotlin 侧按空串处理），绝不 `expect` 跨 FFI。
fn rust_to_jstr<'local>(env: &mut JNIEnv<'local>, s: &str) -> JString<'local> {
    if let Ok(v) = env.new_string(s) {
        return v;
    }
    // 记录一次，供引擎自检可见（note_error 自身永不 panic）。
    crate::engine::note_error("JNI new_string 失败，已降级为空串");
    if let Ok(v) = env.new_string("") {
        return v;
    }
    JString::from(jni::objects::JObject::null())
}

fn guarded<F: FnOnce() -> String>(f: F) -> String {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            engine::note_error("JNI panic（详见 logcat）");
            String::new()
        }
    }
}

fn ok_json(msg: &str) -> String {
    format!("{{\"ok\":true,{msg}}}")
}

fn err_json(e: &str) -> String {
    format!(
        "{{\"ok\":false,\"err\":{}}}",
        serde_json::to_string(e).unwrap_or_else(|_| "\"error\"".to_string())
    )
}

fn join(items: &[String]) -> String {
    items.join(crate::DELIM_STR)
}

/// 候选分析：flag(0=直接 1=纠错 2=纠错记忆) + matched + 候选列表。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_analyzeInput<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &input);
    let out = guarded(|| {
        let m = engine::analyze(&s, 8);
        let flag = if m.remembered {
            "2"
        } else if m.corrected {
            "1"
        } else {
            "0"
        };
        format!("{flag}{FIELD}{}{FIELD}{}", m.matched, join(&m.candidates))
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_initStorage<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jdir: JString<'local>,
) -> JString<'local> {
    let dir = jstr_to_rust(&mut env, &jdir);
    let out = guarded(|| match store::init(&dir) {
        Ok((saved, pins)) => {
            let (lex, ini) = engine::lexicon_info();
            ok_json(&format!(
                "\"saved\":{saved},\"pins\":{pins},\"lex\":{lex},\"ini\":{ini}"
            ))
        }
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_lexiconSize<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| engine::lexicon_size().to_string());
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_candidatesFor<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &input);
    let out = guarded(|| join(&engine::candidates(&s, 8)));
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_pickCandidate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jpinyin: JString<'local>,
    jword: JString<'local>,
    corrected: jboolean,
) -> JString<'local> {
    let p = jstr_to_rust(&mut env, &jpinyin);
    let w = jstr_to_rust(&mut env, &jword);
    let was_corrected = corrected != 0;
    let out = guarded(|| {
        if was_corrected {
            engine::remember(&p, &w);
        }
        let updated = engine::record_pick(&p, &w, 8);
        // 生词本 + 译词缓存（这个词从此不再是生词）
        store::bump_word(&w);
        crate::gloss::clear_cache();
        let _ = store::persist();
        join(&updated)
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_pinCandidate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jpinyin: JString<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let p = jstr_to_rust(&mut env, &jpinyin);
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        let updated = engine::pin(&p, &w, 8);
        let _ = store::persist();
        join(&updated)
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_forgetCandidate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jpinyin: JString<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let p = jstr_to_rust(&mut env, &jpinyin);
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        let updated = engine::forget(&p, &w, 8);
        let _ = store::persist();
        join(&updated)
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_predictNext<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| join(&engine::predict_next(&w, 6)));
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_setEngineOptions<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    fuzzy: jboolean,
    correction: jboolean,
    shuangpin: jint,
    script: jint,
) -> JString<'local> {
    let scheme = u8::try_from(shuangpin).unwrap_or(0);
    let plan = u8::try_from(script).unwrap_or(0);
    // 越界方案静默变全拼会写盘污染设置：直接报错，由设置页提示。
    if scheme > 8 || plan > 1 {
        return rust_to_jstr(&mut env, &err_json("非法的输入方案或字形参数"));
    }
    let out = guarded(
        || match store::set_settings(fuzzy != 0, correction != 0, scheme, plan) {
            Ok(()) => ok_json("\"saved\":1"),
            Err(e) => err_json(&e),
        },
    );
    rust_to_jstr(&mut env, &out)
}

/// 已删词列表（`拼音<RS>词`）
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_blockedWords<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let items: Vec<String> = engine::blocked_words()
            .into_iter()
            .map(|(p, w)| format!("{p}{REC_SEP}{w}"))
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

/// 恢复被删的词
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_unblockWord<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jpinyin: JString<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let p = jstr_to_rust(&mut env, &jpinyin);
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        let n = engine::unblock(&p, &w);
        let _ = store::persist();
        ok_json(&format!("\"restored\":{n}"))
    });
    rust_to_jstr(&mut env, &out)
}

/// 简繁转换：to_trad=true 输出繁体，否则输出简体。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_convertScript<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jtext: JString<'local>,
    to_trad: jboolean,
) -> JString<'local> {
    let text = jstr_to_rust(&mut env, &jtext);
    let out = guarded(|| {
        if to_trad != 0 {
            crate::s2t::to_traditional(&text)
        } else {
            crate::s2t::to_simplified(&text)
        }
    });
    rust_to_jstr(&mut env, &out)
}

/// 记录刚上屏的词（bigram 重排 + trigram 联想）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_setContext<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        engine::set_context(&w);
        ok_json("\"ok\":1")
    });
    rust_to_jstr(&mut env, &out)
}

/// 候选翻页：更多候选（最多 24 条）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_moreCandidates<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &input);
    let out = guarded(|| join(&engine::more_candidates(&s, 24)));
    rust_to_jstr(&mut env, &out)
}

/// 九键候选：数字串 -> 候选。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_t9Candidates<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    digits: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &digits);
    let out = guarded(|| join(&engine::t9_candidates(&s, 8)));
    rust_to_jstr(&mut env, &out)
}

/// 编辑收藏英文（反哺翻译记忆）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_updateSaved<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jcn: JString<'local>,
    jold: JString<'local>,
    jnew: JString<'local>,
) -> JString<'local> {
    let cn = jstr_to_rust(&mut env, &jcn);
    let old = jstr_to_rust(&mut env, &jold);
    let new = jstr_to_rust(&mut env, &jnew);
    let out = guarded(|| match store::update_saved(&cn, &old, &new) {
        Ok(hit) => ok_json(&format!("\"updated\":{hit}")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 删除单条收藏。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_deleteSaved<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jcn: JString<'local>,
    jen: JString<'local>,
) -> JString<'local> {
    let cn = jstr_to_rust(&mut env, &jcn);
    let en = jstr_to_rust(&mut env, &jen);
    let out = guarded(|| match store::delete_saved(&cn, &en) {
        Ok(hit) => ok_json(&format!("\"deleted\":{hit}")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_bumpStats<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jtoday: JString<'local>,
) -> JString<'local> {
    let today = jstr_to_rust(&mut env, &jtoday);
    // 只改内存态（合并写入：落盘由选词时的 persist 与 flushStorage 兜底）
    let out = guarded(|| match store::bump_stats(1, &today) {
        Ok(()) => ok_json(&format!("\"words\":{}", store::stats().words)),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 主动落盘：把内存中的统计计数等写盘（Kotlin 侧在 onFinishInput 调用兜底）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_flushStorage<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| match store::persist_forced() {
        Ok(()) => ok_json("\"saved\":1"),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 用户词库列表：`拼音<RS>词<RS>次数` 列表（次数 0 = 已固定首位）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_learnedWords<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let items: Vec<String> = engine::learned_words()
            .into_iter()
            .map(|(p, w, c)| format!("{p}{REC_SEP}{w}{REC_SEP}{c}"))
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

/// 清空学习记录（置顶与选词计数）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_clearLearned<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let n = engine::clear_learned();
        let _ = store::persist();
        ok_json(&format!("\"cleared\":{n}"))
    });
    rust_to_jstr(&mut env, &out)
}

/// 本地备份：导出整库 JSON。失败返回 `err_json`（调用方必须提示用户），
/// 绝不伪装成空备份覆盖用户手里的旧文件。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_exportBackup<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| match store::export_json() {
        Ok(s) => s,
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 本地恢复：从 JSON 覆盖导入。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_importBackup<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jtext: JString<'local>,
) -> JString<'local> {
    let text = jstr_to_rust(&mut env, &jtext);
    let out = guarded(|| match store::import_json(&text) {
        Ok((saved, pins)) => ok_json(&format!("\"saved\":{saved},\"pins\":{pins}")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 导入姓名（多行/逗号分隔）-> 新增条数
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_importNames<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jtext: JString<'local>,
) -> JString<'local> {
    let text = jstr_to_rust(&mut env, &jtext);
    let out = guarded(|| match store::import_names(&text) {
        Ok(n) => ok_json(&format!("\"added\":{n}")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 个人词库列表（`拼音<RS>姓名`）
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_userWords<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let items: Vec<String> = store::user_words()
            .into_iter()
            .map(|(p, w)| format!("{p}{REC_SEP}{w}"))
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_clearUserWords<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| ok_json(&format!("\"cleared\":{}", store::clear_user_words())));
    rust_to_jstr(&mut env, &out)
}

/// 记住邮箱域名 -> 记忆条数
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_rememberMailDomain<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jdomain: JString<'local>,
) -> JString<'local> {
    let d = jstr_to_rust(&mut env, &jdomain);
    let out = guarded(|| match store::remember_mail_domain(&d) {
        Ok(n) => ok_json(&format!("\"domains\":{n}")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

/// 已记住的邮箱域名（`域名<RS>次数`，按次数降序）
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_mailDomains<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let items: Vec<String> = store::mail_domains()
            .into_iter()
            .map(|(d, c)| format!("{d}{REC_SEP}{c}"))
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_statsInfo<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let st = store::stats();
        let (lex, ini) = engine::lexicon_info();
        let (fuzzy, correction, shuangpin) = engine::options();
        let days = serde_json::to_string(&st.days).unwrap_or_else(|_| "[]".into());
        let s = store::settings();
        ok_json(&format!(
            "\"words\":{},\"days\":{},\"saved\":{},\"lex\":{},\"ini\":{},\"endict\":{},\"biglex\":{},\"sentbank\":{},\"fuzzy\":{},\"correction\":{},\"shuangpin\":{},\"script\":{},\"gloss\":{},\"fresh\":{},\"shortcut\":{},\"englishfix\":{},\"cefr\":{},\"err\":{}",
            st.words,
            days,
            store::saved_count(),
            lex,
            ini,
            english::dict_size(),
            engine::biglex_size(),
            crate::sentbank::size(),
            fuzzy,
            correction,
            shuangpin,
            s.script,
            s.gloss,
            s.fresh_mark,
            s.shortcut,
            s.english_fix,
            crate::gloss::levels_loaded(),
            serde_json::to_string(&engine::last_error_snapshot()).unwrap_or_else(|_| "\"\"".into())
        ))
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_suggestEnglish<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &input);
    let out = guarded(|| english::suggest(&s));
    rust_to_jstr(&mut env, &out)
}

/// 英文候选：每项形如 `kind<RS>text`（RS='\u{1D}'），按优先级排序。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_englishCandidates<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &input);
    let out = guarded(|| {
        let items: Vec<String> = english::english_candidates(&s)
            .into_iter()
            .map(|(kind, text)| format!("{kind}{REC_SEP}{text}"))
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_grammarExplain<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    input: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &input);
    let out = guarded(|| english::grammar_explain(&s));
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_savePhrase<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jcn: JString<'local>,
    jen: JString<'local>,
) -> JString<'local> {
    let cn = jstr_to_rust(&mut env, &jcn);
    let en = jstr_to_rust(&mut env, &jen);
    let out = guarded(|| match store::save_phrase(&cn, &en) {
        Ok(p) => serde_json::to_string(&p).unwrap_or_else(|_| ok_json("\"saved\":1")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_listSaved<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out =
        guarded(|| serde_json::to_string(&store::list_saved()).unwrap_or_else(|_| "[]".into()));
    rust_to_jstr(&mut env, &out)
}

#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_clearSaved<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| match store::clear_saved() {
        Ok(n) => ok_json(&format!("\"cleared\":{n}")),
        Err(e) => err_json(&e),
    });
    rust_to_jstr(&mut env, &out)
}

// ---------------- 多方案 / 逐词译词 / 快捷输入 / 分级 ----------------

/// 可选输入方案清单（设置页直接读，不写死在 Kotlin 里）。
/// 每项 `编号<US>名称`，项与项用 `<GS>` 分隔。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_schemeList<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let items: Vec<String> = crate::shuangpin::schemes()
            .iter()
            .map(|(id, name)| format!("{id}{FIELD}{name}"))
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

/// 候选旁的逐词译词。每项 `词<US>释义<US>词性<US>生词<US>级别`，项间 `<GS>`。
/// 译词功能关掉、或这份候选切不出可靠释义时返回空串（宿主只显示中文）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_glossFor<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        let s = store::settings();
        if !s.gloss {
            return String::new();
        }
        let items: Vec<String> = crate::gloss::segment(&w)
            .iter()
            .map(|p| {
                format!(
                    "{}{FIELD}{}{FIELD}{}{FIELD}{}{FIELD}{}",
                    p.zh,
                    p.en,
                    p.pos,
                    if p.fresh && s.fresh_mark { 1 } else { 0 },
                    p.level
                )
            })
            .collect();
        join(&items)
    });
    rust_to_jstr(&mut env, &out)
}

/// 一整页候选的行内译词：入参 `词<GS>词<GS>…`，出参每词一组
/// `短译<US>是否生词<US>级别`，组间 `<GS>`；没有可靠释义的词该组为空串。
/// 一次 JNI 拿完一页，避免每键跑 8 次跨语言调用。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_glossBatch<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jwords: JString<'local>,
) -> JString<'local> {
    let raw = jstr_to_rust(&mut env, &jwords);
    let out = guarded(|| {
        let s = store::settings();
        if !s.gloss {
            return String::new();
        }
        let groups: Vec<String> = crate::split_delim(&raw)
            .iter()
            .map(|w| match crate::gloss::gloss_line(w) {
                Some((en, level)) => format!(
                    "{en}{FIELD}{}{FIELD}{level}",
                    if s.fresh_mark && store::is_fresh(w) {
                        1
                    } else {
                        0
                    }
                ),
                None => String::new(),
            })
            .collect();
        groups.join(crate::DELIM_STR)
    });
    rust_to_jstr(&mut env, &out)
}

/// 整词短译（「数字键直出译词」用）。没有可靠释义时返回空串。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_shortGloss<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        if !store::settings().gloss {
            return String::new();
        }
        crate::gloss::short_gloss(&w).unwrap_or_default()
    });
    rust_to_jstr(&mut env, &out)
}

/// 英文模式拼写纠正：把打错的英文词纠正成词表里的真实词；不该改则返回空串。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_fixSpelling<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jword: JString<'local>,
) -> JString<'local> {
    let w = jstr_to_rust(&mut env, &jword);
    let out = guarded(|| {
        if !store::settings().english_fix {
            return String::new();
        }
        crate::english::fix_spelling(&w).unwrap_or_default()
    });
    rust_to_jstr(&mut env, &out)
}

/// 快捷输入展开：命中返回 `内容<US>分类`（算式/中文数字/金额/字符），否则空串。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_expandShortcut<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    jinput: JString<'local>,
) -> JString<'local> {
    let s = jstr_to_rust(&mut env, &jinput);
    let out = guarded(|| {
        if !store::settings().shortcut {
            return String::new();
        }
        match crate::shortcut::expand(&s) {
            Some(h) => format!("{}{FIELD}{}", h.text, h.kind),
            None => String::new(),
        }
    });
    rust_to_jstr(&mut env, &out)
}

/// 词级分级统计：`{"ok":true,"levels":[["A1",312],…],"terms":12345}`。
/// 词表没载入时 `levels` 为空数组——不编数字。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_vocabLevels<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let rows: Vec<(String, usize)> = crate::gloss::vocab_stats();
        let arr: Vec<String> = rows
            .iter()
            .map(|(k, v)| format!("[{},{}]", serde_json::to_string(k).unwrap_or_default(), v))
            .collect();
        ok_json(&format!(
            "\"levels\":[{}],\"terms\":{}",
            arr.join(","),
            crate::gloss::levels_loaded()
        ))
    });
    rust_to_jstr(&mut env, &out)
}

/// 写入功能开关（逐词译词 / 生词橙标 / 快捷输入 / 英文纠错）。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_setFeatureOptions<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    gloss: jboolean,
    fresh_mark: jboolean,
    shortcut: jboolean,
    english_fix: jboolean,
) -> JString<'local> {
    let out = guarded(|| {
        match store::set_features(gloss != 0, fresh_mark != 0, shortcut != 0, english_fix != 0) {
            Ok(()) => {
                crate::gloss::clear_cache();
                ok_json("\"saved\":1")
            }
            Err(e) => err_json(&e),
        }
    });
    rust_to_jstr(&mut env, &out)
}

/// 错误历史（环形 8 条）：`[{"t":ts,"m":"msg"},…]`，供设置页「引擎自检」一次展示。
#[no_mangle]
pub extern "system" fn Java_com_typesake_app_TypesakeCore_errorHistory<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    let out = guarded(|| {
        let items: Vec<String> = engine::error_history()
            .into_iter()
            .map(|(t, m)| {
                format!(
                    "{{\"t\":{t},\"m\":{}}}",
                    serde_json::to_string(&m).unwrap_or_else(|_| "\"\"".to_string())
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    });
    rust_to_jstr(&mut env, &out)
}
