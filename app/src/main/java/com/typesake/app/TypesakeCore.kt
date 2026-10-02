package com.typesake.app

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/**
 * JNI 桥：优先调 Rust `libtypesake_core.so`，缺库时用 Kotlin 兜底。
 *
 * 协议：热路径返回分隔串（US='\u{1E}' 字段分隔，GS='\u{1F}' 列表分隔），
 * 冷路径（收藏/统计/初始化）返回 JSON。所有 JNI 在 Rust 侧都有 catch_unwind。
 */
object TypesakeCore {
    @Serializable
    data class SavedPhrase(
        val chinese: String = "",
        val english: String = "",
        val saved_at: Long = 0L,
    )

    /** 一次输入的分析结果。 */
    data class Match(
        val matched: String,
        val corrected: Boolean,
        val remembered: Boolean,
        val candidates: List<String>,
    )

    /** 英文候选：来源标签（我的/地道/结构/直译）+ 文本。 */
    data class EnglishOption(val kind: String, val text: String) {
        val kindLabel: String
            get() = when (kind) {
                KIND_MINE -> "我的"
                KIND_NATIVE -> "地道"
                KIND_PATTERN -> "结构"
                KIND_LITERAL -> "直译"
                KIND_SENTBANK -> "句库"
                else -> ""
            }
    }

    data class Stats(
        val words: Long = 0L,
        val days: List<String> = emptyList(),
        val saved: Int = 0,
        val lex: Long = 0L,
        val initials: Int = 0,
        val endict: Int = 0,
        val biglex: Int = 0,
        val sentbank: Int = 0,
        val shuangpin: Int = 0,
        val lastError: String = "",
        val fuzzy: Boolean = true,
        val correction: Boolean = true,
    )

    private const val DELIM = '\u001F'
    private const val FIELD = '\u001E'
    private const val RS = '\u001D'

    const val KIND_MINE = "1"
    const val KIND_NATIVE = "2"
    const val KIND_PATTERN = "3"
    const val KIND_LITERAL = "4"
    const val KIND_SENTBANK = "6"
    private const val CACHE_MAX = 64

    private val json = Json { ignoreUnknownKeys = true }

    /** 访问序 LRU（不用 android.util.LruCache，便于 JVM 单测）。 */
    private val cache = object : LinkedHashMap<String, List<String>>(CACHE_MAX, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, List<String>>): Boolean =
            size > CACHE_MAX
    }

    var available: Boolean = false
        private set

    private var lexEntries: Long = 0L

    init {
        available = try {
            System.loadLibrary("typesake_core")
            true
        } catch (_: UnsatisfiedLinkError) {
            false
        }
    }

    // ---- JNI（Rust #[no_mangle] 实现） ----
    @JvmStatic private external fun initStorage(dir: String): String
    @JvmStatic private external fun lexiconSize(): String
    @JvmStatic private external fun analyzeInput(input: String): String
    @JvmStatic private external fun candidatesFor(input: String): String
    @JvmStatic private external fun pickCandidate(pinyin: String, word: String, corrected: Boolean): String
    @JvmStatic private external fun pinCandidate(pinyin: String, word: String): String
    @JvmStatic private external fun forgetCandidate(pinyin: String, word: String): String
    @JvmStatic private external fun predictNext(word: String): String
    @JvmStatic private external fun setEngineOptions(fuzzy: Boolean, correction: Boolean, shuangpin: Int, script: Int): String
    @JvmStatic private external fun convertScript(text: String, toTrad: Boolean): String
    @JvmStatic private external fun setContext(word: String): String
    @JvmStatic private external fun moreCandidates(input: String): String
    @JvmStatic private external fun t9Candidates(digits: String): String
    @JvmStatic private external fun updateSaved(chinese: String, oldEnglish: String, newEnglish: String): String
    @JvmStatic private external fun deleteSaved(chinese: String, english: String): String
    @JvmStatic private external fun learnedWords(): String
    @JvmStatic private external fun clearLearned(): String
    @JvmStatic private external fun exportBackup(): String
    @JvmStatic private external fun importBackup(text: String): String
    @JvmStatic private external fun importNames(text: String): String
    @JvmStatic private external fun userWords(): String
    @JvmStatic private external fun clearUserWords(): String
    @JvmStatic private external fun rememberMailDomain(domain: String): String
    @JvmStatic private external fun mailDomains(): String
    @JvmStatic private external fun blockedWords(): String
    @JvmStatic private external fun unblockWord(pinyin: String, word: String): String
    @JvmStatic private external fun bumpStats(today: String): String
    @JvmStatic private external fun flushStorage(): String
    @JvmStatic private external fun statsInfo(): String
    @JvmStatic private external fun suggestEnglish(chinese: String): String
    @JvmStatic private external fun englishCandidates(chinese: String): String
    @JvmStatic private external fun grammarExplain(english: String): String
    @JvmStatic private external fun savePhrase(chinese: String, english: String): String
    @JvmStatic private external fun listSaved(): String
    @JvmStatic private external fun clearSaved(): String
    @JvmStatic private external fun schemeList(): String
    @JvmStatic private external fun glossFor(word: String): String
    @JvmStatic private external fun glossBatch(words: String): String
    @JvmStatic private external fun shortGloss(word: String): String
    @JvmStatic private external fun fixSpelling(word: String): String
    @JvmStatic private external fun expandShortcut(input: String): String
    @JvmStatic private external fun vocabLevels(): String
    @JvmStatic private external fun setFeatureOptions(
        gloss: Boolean,
        freshMark: Boolean,
        shortcut: Boolean,
        englishFix: Boolean,
    ): String

    // ---- 解析工具（internal 供单测） ----

    internal fun splitDelim(s: String): List<String> =
        if (s.isEmpty()) emptyList() else s.split(DELIM).filter { it.isNotEmpty() }

    internal fun intField(jsonish: String, key: String): Int =
        Regex("\"$key\":(\\d+)").find(jsonish)?.groupValues?.get(1)?.toIntOrNull() ?: 0

    internal fun longField(jsonish: String, key: String): Long =
        Regex("\"$key\":(\\d+)").find(jsonish)?.groupValues?.get(1)?.toLongOrNull() ?: 0L

    internal fun boolField(jsonish: String, key: String): Boolean =
        Regex("\"$key\":(true|false)").find(jsonish)?.groupValues?.get(1) == "true"

    internal fun stringField(jsonish: String, key: String): String =
        Regex("\"$key\":\"([^\"]*)\"").find(jsonish)?.groupValues?.get(1) ?: ""

    internal fun stringArrayField(jsonish: String, key: String): List<String> {
        val body = Regex("\"$key\":\\[(.*?)]").find(jsonish)?.groupValues?.get(1) ?: return emptyList()
        return Regex("\"([^\"]*)\"").findAll(body).map { it.groupValues[1] }.toList()
    }

    /** 解析 analyzeInput 的三段协议。 */
    internal fun parseMatch(raw: String, fallbackPinyin: String): Match {
        val parts = raw.split(FIELD)
        if (parts.size < 3) {
            return Match(fallbackPinyin, false, false, splitDelim(raw))
        }
        val flag = parts[0]
        return Match(
            matched = parts[1].ifEmpty { fallbackPinyin },
            corrected = flag == "1" || flag == "2",
            remembered = flag == "2",
            candidates = splitDelim(parts[2]),
        )
    }

    // ---- 公开 API ----

    /** 初始化存储目录，返回已载入收藏数。 */
    fun init(dir: String): Int {
        if (!available) return 0
        return try {
            val raw = initStorage(dir)
            lexEntries = longField(raw, "lex")
            intField(raw, "saved")
        } catch (_: Exception) {
            0
        }
    }

    fun lexiconEntries(): Long {
        if (!available) return 0L
        if (lexEntries > 0) return lexEntries
        lexEntries = try {
            lexiconSize().toLongOrNull() ?: 0L
        } catch (_: Exception) {
            0L
        }
        return lexEntries
    }

    fun cached(pinyin: String): List<String>? = synchronized(cache) { cache[pinyin] }

    /** 完整分析：候选 + 纠错信息（每键调用一次）。 */
    fun analyze(pinyin: String): Match {
        if (!available) {
            return Match(pinyin, false, false, fallbackCandidates(pinyin))
        }
        return runCatching { parseMatch(analyzeInput(pinyin), pinyin) }
            .getOrDefault(Match(pinyin, false, false, fallbackCandidates(pinyin)))
    }

    fun candidates(pinyin: String): List<String> =
        if (!available) fallbackCandidates(pinyin)
        else runCatching {
            val list = splitDelim(candidatesFor(pinyin))
            synchronized(cache) { cache[pinyin] = list }
            list
        }.getOrDefault(fallbackCandidates(pinyin))

    /** 上报选词（词频学习 + 纠错记忆），返回更新后的候选序。 */
    fun pick(pinyin: String, word: String, corrected: Boolean = false): List<String> {
        if (!available) return cached(pinyin) ?: fallbackCandidates(pinyin)
        return runCatching {
            val updated = splitDelim(pickCandidate(pinyin, word, corrected))
            synchronized(cache) { cache[pinyin] = updated }
            updated
        }.getOrDefault(emptyList())
    }

    /** 置顶（用户主动固定首位）。 */
    fun pin(pinyin: String, word: String): List<String> {
        if (!available) return cached(pinyin) ?: fallbackCandidates(pinyin)
        return runCatching {
            val updated = splitDelim(pinCandidate(pinyin, word))
            synchronized(cache) { cache[pinyin] = updated }
            updated
        }.getOrDefault(emptyList())
    }

    /** 删词：该拼音下不再推荐此词。 */
    fun forget(pinyin: String, word: String): List<String> {
        if (!available) return cached(pinyin) ?: fallbackCandidates(pinyin)
        return runCatching {
            val updated = splitDelim(forgetCandidate(pinyin, word))
            synchronized(cache) { cache[pinyin] = updated }
            updated
        }.getOrDefault(emptyList())
    }

    fun predict(word: String): List<String> =
        if (!available) emptyList()
        else runCatching { splitDelim(predictNext(word)) }.getOrDefault(emptyList())

    /** 写入模糊音/击键纠错/双拼 选项（会落盘）。 */
    fun setOptions(fuzzy: Boolean, correction: Boolean, shuangpin: Int = 0, script: Int = 0) {
        if (!available) return
        runCatching { setEngineOptions(fuzzy, correction, shuangpin, script) }
        synchronized(cache) { cache.clear() }
    }

    /** 简繁转换（本地 OpenCC 表）。 */
    fun convert(text: String, toTrad: Boolean): String {
        if (!available || text.isEmpty()) return text
        return runCatching { convertScript(text, toTrad) }.getOrDefault(text)
    }

    /** 记录刚上屏的词：bigram 重排 + trigram 联想。 */
    fun context(word: String) {
        if (!available || word.isEmpty()) return
        runCatching { setContext(word) }
    }

    /** 候选翻页：更多候选。 */
    fun more(input: String): List<String> =
        if (!available) cached(input) ?: fallbackCandidates(input)
        else runCatching { splitDelim(moreCandidates(input)) }.getOrDefault(emptyList())

    /** 九键候选。 */
    fun t9(digits: String): List<String> =
        if (!available) fallbackCandidates(digits)
        else runCatching { splitDelim(t9Candidates(digits)) }.getOrDefault(emptyList())

    // ---- 逐词译词 / 多方案 / 快捷输入 / 分级 ----

    /** 一个可选输入方案。 */
    data class Scheme(val id: Int, val name: String)

    /** 可选方案清单（0 全拼 … 8 大千注音），来源是引擎，不在 Kotlin 里写死。 */
    fun schemes(): List<Scheme> {
        if (!available) return listOf(Scheme(0, "全拼"))
        return runCatching {
            splitDelim(schemeList()).mapNotNull { item ->
                val p = item.split(FIELD)
                if (p.size < 2) null else Scheme(p[0].toIntOrNull() ?: return@mapNotNull null, p[1])
            }
        }.getOrDefault(listOf(Scheme(0, "全拼")))
    }

    /** 候选行内的一行译词。`level` 为空表示词表没收录该词。 */
    data class GlossLine(val text: String, val fresh: Boolean, val level: String)

    /**
     * 一次取一整页候选的行内译词。返回列表与入参**等长**，
     * 没有可靠释义的位置是 null（调用方据此决定不显示译词，而不是显示猜的）。
     */
    fun glossLines(words: List<String>): List<GlossLine?> {
        if (!available || words.isEmpty()) return words.map { null }
        return runCatching {
            val groups = glossBatch(words.joinToString(DELIM.toString())).split(DELIM)
            words.mapIndexed { i, _ ->
                val g = groups.getOrNull(i).orEmpty()
                if (g.isEmpty()) null else {
                    val p = g.split(FIELD)
                    GlossLine(
                        text = p.getOrNull(0).orEmpty(),
                        fresh = p.getOrNull(1) == "1",
                        level = p.getOrNull(2).orEmpty(),
                    ).takeIf { it.text.isNotEmpty() }
                }
            }
        }.getOrDefault(words.map { null })
    }

    /** 逐词译词明细（长按候选时展示：每段的 中文 / 释义 / 词性 / 生词 / 级别）。 */
    data class GlossWord(
        val zh: String,
        val en: String,
        val pos: String,
        val fresh: Boolean,
        val level: String,
    )

    fun glossDetail(word: String): List<GlossWord> {
        if (!available || word.isEmpty()) return emptyList()
        return runCatching {
            splitDelim(glossFor(word)).mapNotNull { item ->
                val p = item.split(FIELD)
                if (p.size < 5) null else GlossWord(
                    zh = p[0], en = p[1], pos = p[2],
                    fresh = p[3] == "1", level = p[4],
                )
            }
        }.getOrDefault(emptyList())
    }

    /** 整词短译（数字键直出译词用）；没有可靠释义返回空串。 */
    fun shortGlossOf(word: String): String =
        if (!available) "" else runCatching { shortGloss(word) }.getOrDefault("")

    /**
     * 英文模式拼写纠正：把打错的词换成词表里的真实词，不该改时返回空串。
     * 引擎侧按 `english_fix` 开关生效，这里只负责兜住未加载的情形。
     */
    fun spellingFix(word: String): String =
        if (!available) "" else runCatching { fixSpelling(word) }.getOrDefault("")

    /** 快捷输入命中。 */
    data class Shortcut(val text: String, val kind: String)

    /** `v1+2` / `i123` / `im123.45` / `u4e00` 命中时返回展开结果，否则 null。 */
    fun shortcutOf(input: String): Shortcut? {
        if (!available) return null
        return runCatching {
            val raw = expandShortcut(input)
            if (raw.isEmpty()) null else {
                val p = raw.split(FIELD)
                if (p.isEmpty() || p[0].isEmpty()) null else Shortcut(p[0], p.getOrNull(1).orEmpty())
            }
        }.getOrDefault(null)
    }

    /** 词级分级统计：`[("A1", 312), …]`。词表没载入时为空。 */
    fun vocabLevelStats(): List<Pair<String, Int>> {
        if (!available) return emptyList()
        return runCatching {
            Regex("\\[\"([ABC][12])\",(\\d+)]")
                .findAll(vocabLevels())
                .map { m -> m.groupValues[1] to (m.groupValues[2].toIntOrNull() ?: 0) }
                .toList()
        }.getOrDefault(emptyList())
    }

    /** 写入功能开关（逐词译词 / 生词橙标 / 快捷输入 / 英文纠错），会落盘。 */
    fun setFeatures(gloss: Boolean, freshMark: Boolean, shortcut: Boolean, englishFix: Boolean) {
        if (!available) return
        runCatching { setFeatureOptions(gloss, freshMark, shortcut, englishFix) }
    }

    /** 编辑收藏英文（反哺翻译记忆）。 */
    fun editSaved(chinese: String, oldEnglish: String, newEnglish: String): Boolean {
        if (!available) return MemStore.update(chinese, oldEnglish, newEnglish)
        return runCatching { intField(updateSaved(chinese, oldEnglish, newEnglish), "updated") == 1 }
            .getOrDefault(false)
    }

    /** 用户词库条目：拼音 / 词 / 选词次数（0 = 已固定首位）。 */
    data class LearnedWord(val pinyin: String, val word: String, val picks: Int)

    /** 用户词库列表（本地学习记录）。注意与 JNI 同名，故公开 API 另行命名。 */
    fun myWords(): List<LearnedWord> {
        if (!available) return emptyList()
        return runCatching {
            splitDelim(learnedWords()).mapNotNull { item ->
                val parts = item.split(RS)
                if (parts.size < 2) null else LearnedWord(
                    parts[0],
                    parts[1],
                    parts.getOrNull(2)?.toIntOrNull() ?: 0,
                )
            }
        }.getOrDefault(emptyList())
    }

    /** 清空学习记录（保留收藏）。 */
    fun resetLearning(): Int {
        if (!available) return 0
        return runCatching { intField(clearLearned(), "cleared") }.getOrDefault(0)
    }

    /** 本地备份：整库 JSON（可分享/保存）。 */
    fun backupJson(): String =
        if (!available) "" else runCatching { exportBackup() }.getOrDefault("")

    /** 本地恢复：从 JSON 覆盖导入。 */
    fun restoreJson(text: String): Boolean {
        if (!available || text.isBlank()) return false
        return runCatching { !importBackup(text).contains("\"ok\":false") }.getOrDefault(false)
    }

    /** 导入姓名（换行/逗号/空格分隔）返回新增条数。 */
    fun addNames(text: String): Int =
        if (!available) 0 else runCatching { intField(importNames(text), "added") }.getOrDefault(0)

    /** 个人词库（拼音 -> 姓名）。 */
    fun myNames(): List<Pair<String, String>> {
        if (!available) return emptyList()
        return runCatching {
            splitDelim(userWords()).mapNotNull { item ->
                val i = item.indexOf(RS)
                if (i <= 0) null else item.substring(0, i) to item.substring(i + 1)
            }
        }.getOrDefault(emptyList())
    }

    fun clearNames(): Int =
        if (!available) 0 else runCatching { intField(clearUserWords(), "cleared") }.getOrDefault(0)

    /** 记住邮箱域名（点过就记住，下次优先）。 */
    fun recallMailDomain(domain: String) {
        if (!available || domain.isEmpty()) return
        runCatching { rememberMailDomain(domain) }
    }

    /** 已记住的邮箱域名（按次数降序）。 */
    fun learnedMailDomains(): List<String> {
        if (!available) return emptyList()
        return runCatching {
            splitDelim(mailDomains()).mapNotNull { item ->
                val i = item.indexOf(RS)
                if (i <= 0) null else item.substring(0, i)
            }
        }.getOrDefault(emptyList())
    }

    /** 已删词黑名单（可恢复）。 */
    fun blockedList(): List<Pair<String, String>> {
        if (!available) return emptyList()
        return runCatching {
            splitDelim(blockedWords()).mapNotNull { item ->
                val i = item.indexOf(RS)
                if (i <= 0) null else item.substring(0, i) to item.substring(i + 1)
            }
        }.getOrDefault(emptyList())
    }

    /** 恢复被删的词。 */
    fun restoreWord(pinyin: String, word: String): Boolean {
        if (!available) return false
        return runCatching { intField(unblockWord(pinyin, word), "restored") > 0 }.getOrDefault(false)
    }

    /** 删除单条收藏。 */
    fun removeSaved(chinese: String, english: String): Boolean {
        if (!available) return MemStore.delete(chinese, english)
        return runCatching { intField(deleteSaved(chinese, english), "deleted") == 1 }
            .getOrDefault(false)
    }

    /** 记一次上屏（词数 + 当天活跃）。只改内存态，落盘由选词时的写入与 [flush] 兜底。 */
    fun bump(today: String) {
        if (!available) return
        runCatching { bumpStats(today) }
    }

    /** 把内存态（统计计数等）落盘；在输入收尾（onFinishInput）时调用。 */
    fun flush() {
        if (!available) return
        runCatching { flushStorage() }
    }

    fun stats(): Stats {
        if (!available) {
            return Stats(saved = MemStore.list().size)
        }
        return runCatching {
            val raw = statsInfo()
            Stats(
                words = longField(raw, "words"),
                days = stringArrayField(raw, "days"),
                saved = intField(raw, "saved"),
                lex = longField(raw, "lex"),
                initials = intField(raw, "ini"),
                endict = intField(raw, "endict"),
                biglex = intField(raw, "biglex"),
                sentbank = intField(raw, "sentbank"),
                shuangpin = intField(raw, "shuangpin"),
                lastError = stringField(raw, "err"),
                fuzzy = boolField(raw, "fuzzy"),
                correction = boolField(raw, "correction"),
            )
        }.getOrDefault(Stats(saved = MemStore.list().size))
    }

    fun suggest(chinese: String): String =
        if (!available) fallbackSuggest(chinese)
        else runCatching { suggestEnglish(chinese) }.getOrDefault(fallbackSuggest(chinese))

    /** 解析 `kind<RS>text` 列表。 */
    internal fun parseEnglish(raw: String): List<EnglishOption> =
        splitDelim(raw).mapNotNull { item ->
            val i = item.indexOf(RS)
            if (i <= 0) null else EnglishOption(item.substring(0, i), item.substring(i + 1))
        }

    fun englishList(chinese: String): List<EnglishOption> {
        if (!available) {
            return fallbackSuggest(chinese).takeIf { it.isNotEmpty() }
                ?.let { listOf(EnglishOption(KIND_NATIVE, it)) } ?: emptyList()
        }
        return runCatching { parseEnglish(englishCandidates(chinese)) }.getOrDefault(emptyList())
    }

    fun explain(english: String): String =
        if (!available) fallbackExplain(english)
        else runCatching { grammarExplain(english) }.getOrDefault(fallbackExplain(english))

    /** 收藏（中文必填，英文可留空待补）。 */
    fun save(chinese: String, english: String): Boolean {
        if (chinese.isBlank()) return false
        if (!available) return MemStore.save(chinese.trim(), english.trim())
        return runCatching { savePhrase(chinese, english); true }.getOrDefault(false)
    }

    fun list(): List<SavedPhrase> =
        if (!available) MemStore.list()
        else runCatching { json.decodeFromString<List<SavedPhrase>>(listSaved()) }
            .getOrDefault(MemStore.list())

    fun clear(): Int =
        if (!available) MemStore.clear()
        else runCatching { intField(clearSaved(), "cleared") }.getOrDefault(MemStore.clear())

    // ---- 无 .so 时的最小兜底（演示/降级） ----
    private val pinMap = mapOf(
        "nihao" to listOf("你好"), "ni" to listOf("你", "泥", "尼", "呢"),
        "hao" to listOf("好", "号"), "xiexie" to listOf("谢谢"),
        "zaijian" to listOf("再见"), "zhongguo" to listOf("中国"),
        "yingyu" to listOf("英语"), "gongzuo" to listOf("工作"),
        "huiyi" to listOf("会议"), "pengyou" to listOf("朋友"),
        "jintian" to listOf("今天"), "mingtian" to listOf("明天"),
    )
    private val enMap = mapOf(
        "你好" to "Hello!", "谢谢" to "Thank you!", "再见" to "Goodbye! / See you!",
        "中国" to "China", "英语" to "English", "工作" to "work / job",
        "会议" to "meeting", "朋友" to "friend", "学习英语" to "Learn English",
        "今天" to "today", "明天" to "tomorrow",
    )

    private fun fallbackCandidates(p: String): List<String> {
        val k = p.lowercase().replace(" ", "")
        if (k.isEmpty()) return emptyList()
        pinMap[k]?.let { return it }
        if (k.length == 2 && k.all { it in "bcdfghjklmnpqrstvwxyz" }) {
            val expanded = k.map { ch ->
                pinMap.keys.firstOrNull { it.startsWith(ch) } ?: ""
            }
            val phrase = expanded.joinToString("")
            if (phrase.isNotEmpty()) {
                pinMap[phrase]?.let { return it }
            }
        }
        return pinMap.entries.firstOrNull { it.key.startsWith(k) || k.startsWith(it.key) }?.value
            ?: emptyList()
    }

    private fun fallbackSuggest(c: String): String =
        enMap[c.trim()] ?: enMap.entries.firstOrNull { c.contains(it.key) }?.value ?: ""

    private fun fallbackExplain(e: String): String =
        "句子：$e\n句式：${if (e.trimEnd().endsWith("?")) "疑问句" else "陈述句"}\n提示：接上 Rust .so 后可看完整时态与成分分析。"

    /** 进程内兜底收藏（仅无 .so 时用）。 */
    private object MemStore {
        private val items = mutableListOf<SavedPhrase>()

        @Synchronized
        fun save(c: String, e: String): Boolean {
            items.removeAll { it.chinese == c && it.english == e }
            items.add(0, SavedPhrase(c, e, System.currentTimeMillis() / 1000))
            return true
        }

        @Synchronized
        fun list(): List<SavedPhrase> = items.toList()

        @Synchronized
        fun clear(): Int = items.size.also { items.clear() }

        @Synchronized
        fun delete(c: String, e: String): Boolean = items.removeAll { it.chinese == c && it.english == e }

        @Synchronized
        fun update(c: String, oldEn: String, newEn: String): Boolean {
            val i = items.indexOfFirst { it.chinese == c && it.english == oldEn }
            if (i < 0) return false
            items[i] = items[i].copy(english = newEn, saved_at = System.currentTimeMillis() / 1000)
            return true
        }
    }
}
