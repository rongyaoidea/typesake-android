package com.typesake.app

import java.time.LocalDate

/** 纯文本工具（无 Android 依赖，便于 JVM 单测）。 */
object TextUtils {

    private val TERMINATORS = charArrayOf(
        '。', '！', '？', '!', '?', ';', '；', '\n', '.',
    )

    /**
     * 取光标前文本里的“当前句”：最后一个句末符号之后的内容。
     * 主要用于双击空格 / ★ 收藏整句。
     */
    fun sentenceBefore(text: String, max: Int = 120): String {
        if (text.isBlank()) return ""
        var start = 0
        for (i in text.indices) {
            if (text[i] in TERMINATORS) start = i + 1
        }
        var out = text.substring(start).trim()
        if (out.length > max) out = out.takeLast(max)
        return out
    }

    /** 词块边界：空白 + 常见中英标点（比句末符号更宽）。 */
    private const val WORD_BOUNDARY = "，。！？；：、,.!?;:…—（）()《》「」【】\"'“”‘’"

    /** 光标前文本里“当前词块”的长度（滑动删除用）：跳过尾部空白/标点，再回退到上一个边界。 */
    fun deleteWordLength(textBefore: String, max: Int = 40): Int {
        if (textBefore.isEmpty()) return 0
        val slice = textBefore.takeLast(max)
        var i = slice.length
        while (i > 0 && (slice[i - 1].isWhitespace() || slice[i - 1] in WORD_BOUNDARY)) i--
        val end = i
        while (i > 0 && !slice[i - 1].isWhitespace() && slice[i - 1] !in WORD_BOUNDARY) i--
        val len = end - i
        return if (len > 0) len else 1
    }

    /**
     * 智能标点：前一个字符是 ASCII 字母/数字时用英文标点（小数、网址、代码场景），
     * 否则用中文全角标点。
     */
    fun smartPunctuation(ch: Char, before: Char?): String {
        val asciiContext = before != null && (before.isDigit() || (before.isLetter() && before.code < 128))
        return when (ch) {
            ',' -> if (asciiContext) "," else "，"
            '.' -> if (asciiContext) "." else "。"
            '?' -> if (asciiContext) "?" else "？"
            '!' -> if (asciiContext) "!" else "！"
            ';' -> if (asciiContext) ";" else "；"
            ':' -> if (asciiContext) ":" else "："
            else -> ch.toString()
        }
    }

    private val PAIRS: Map<Char, Pair<String, String>> = mapOf(
        '(' to ("(" to ")"),
        '[' to ("[" to "]"),
        '{' to ("{" to "}"),
        '"' to ("\u201C" to "\u201D"),
        '\'' to ("\u2018" to "\u2019"),
        '《' to ("《" to "》"),
        '「' to ("「" to "」"),
        '【' to ("【" to "】"),
    )

    fun insertPairFor(ch: Char): Pair<String, String>? = PAIRS[ch]

    fun pairCloseFor(open: String): String? = PAIRS.values.firstOrNull { it.first == open }?.second

    fun isPairClose(ch: Char): Boolean = PAIRS.values.any { it.second.length == 1 && it.second[0] == ch }

    /**
     * 数字智能候选：手机号 / 日期 / 时间的成对格式（对齐搜狗的智能识别）。
     * 只对纯数字串生效。
     */
    fun digitCandidates(raw: String): List<String> {
        val d = raw.trim()
        if (d.isEmpty() || !d.all { it.isDigit() }) return emptyList()
        val out = mutableListOf<String>()
        when {
            d.length == 11 && d.startsWith("1") -> {
                out += "${d.substring(0, 3)} ${d.substring(3, 7)} ${d.substring(7)}"
                out += "${d.substring(0, 3)}-${d.substring(3, 7)}-${d.substring(7)}"
            }
            d.length == 8 && (d.startsWith("19") || d.startsWith("20")) -> {
                val y = d.substring(0, 4).toIntOrNull() ?: 0
                val m = d.substring(4, 6).toIntOrNull() ?: 0
                val day = d.substring(6, 8).toIntOrNull() ?: 0
                if (m in 1..12 && day in 1..31) out += "${y}年${m}月${day}日"
            }
            d.length == 4 -> {
                val h = d.substring(0, 2).toIntOrNull() ?: -1
                val mi = d.substring(2, 4).toIntOrNull() ?: -1
                if (h in 0..23 && mi in 0..59) out += "${d.substring(0, 2)}:${d.substring(2, 4)}"
            }
        }
        return out
    }

    /** 单位拼音 -> 中文（数字/日期混合识别用） */
    private val DIGIT_UNITS: Map<String, String> = mapOf(
        "nian" to "年", "yue" to "月", "ri" to "日", "hao" to "号",
        "dian" to "点", "fen" to "分", "miao" to "秒",
        "tian" to "天", "zhou" to "周", "sui" to "岁",
        "ge" to "个", "kuai" to "块", "yuan" to "元", "ren" to "人",
        "shi" to "时", "fen2" to "分",
    )

    private val MIXED_TOKEN = Regex("(\\d+)([a-z]+)?")
    private val AT_DOMAIN = Regex("@([a-z0-9.]{1,})$")
    private val HOST_DOT = Regex("([a-z0-9-]{2,})\\.$")

    /**
     * 数字/日期混合识别：`2013nian10yue1ri` → 2013年10月1日；`3dian8fen` → 3点8分。
     * 返回 (替换文本, 需要删除的尾部长度)；识别不出返回 null。
     */
    fun mixedDigitSuggestion(textBefore: String): Pair<String, Int>? {
        val tail = textBefore.takeLast(28)
        val lower = tail.lowercase()
        val tokens = MIXED_TOKEN.findAll(lower).toList()
        if (tokens.isEmpty()) return null
        // 尾部必须正好由这些 token 连续组成
        val consumed = tokens.joinToString("") { it.value }
        if (consumed.isEmpty() || !lower.endsWith(consumed)) return null
        val out = StringBuilder()
        var units = 0
        for (m in tokens) {
            val digits = m.groupValues[1]
            val unit = m.groupValues[2]
            out.append(digits)
            if (unit.isNotEmpty()) {
                val zh = DIGIT_UNITS[unit] ?: return null
                out.append(zh)
                units++
            }
        }
        if (units == 0) return null
        return out.toString() to consumed.length
    }

    /** 邮箱后缀联想：文本以 `@单词` 结尾时给出常见域名。 */
    fun emailDomainCandidates(
        textBefore: String,
        learned: List<String> = emptyList(),
    ): List<Pair<String, String>> {
        val m = AT_DOMAIN.find(textBefore.lowercase()) ?: return emptyList()
        val typed = m.groupValues[1]
        val out = mutableListOf<Pair<String, String>>()
        val pool = learned + listOf("gmail.com", "outlook.com", "qq.com", "163.com", "foxmail.com")
        for (domain in pool) {
            if (domain != typed && domain.startsWith(typed) && out.none { it.first == domain }) {
                out += domain to domain.substring(typed.length)
                if (out.size >= 3) break
            }
        }
        return out
    }

    /** 网址后缀联想：文本以 `单词.` 结尾时给出常见域名后缀。 */
    fun domainSuffixCandidates(textBefore: String): List<Pair<String, String>> {
        val m = HOST_DOT.find(textBefore.lowercase()) ?: return emptyList()
        return listOf("com", "cn", "com.cn", "org", "net", "io").take(3).map { it to it }
    }

    /** 连续打卡天数（今天没输入时从昨天往前算）。 */
    fun streak(days: List<String>, today: String): Int {
        val set = days.toHashSet()
        if (set.isEmpty()) return 0
        var cursor = runCatching { LocalDate.parse(today) }.getOrNull() ?: return 0
        if (!set.contains(cursor.toString())) cursor = cursor.minusDays(1)
        var count = 0
        while (set.contains(cursor.toString())) {
            count++
            cursor = cursor.minusDays(1)
        }
        return count
    }
}
