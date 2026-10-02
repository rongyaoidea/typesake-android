package com.typesake.app

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.drawable.GradientDrawable
import android.inputmethodservice.InputMethodService
import android.os.SystemClock
import android.text.InputType
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import android.text.style.RelativeSizeSpan
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.PopupWindow
import android.widget.TextView
import android.widget.Toast
import com.typesake.app.kb.KbColors
import com.typesake.app.kb.KbKind
import com.typesake.app.kb.KbLayer
import com.typesake.app.kb.KbLayouts
import com.typesake.app.kb.KbThemes
import com.typesake.app.kb.KeyboardView
import com.typesake.app.kb.OneHand
import java.time.LocalDate
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Typesake 输入法服务。
 *
 * 布局（对齐主流中文输入法）：**中文候选在上，英文伴学在下**，再往下是键盘。
 * 体验要点：
 * - 拼音走 composing region；候选来自 Rust 引擎（精确/整句/前缀/简拼/模糊音/击键纠错）
 * - 纠错命中时标签显示 `输入→纠正` 并在候选上打「纠」标；用户选中后记住该错拼
 * - 长按候选：置顶 / 删词；长按 ⌫ 或左滑 ⌫：删词块；空格左右滑：移光标
 * - 英文条点按直接上屏（中英混输），长按把刚上屏的中文改写成英文
 */
class TypesakeImeService : InputMethodService(), KeyboardView.Listener {

    private lateinit var prefs: TypesakePrefs
    private lateinit var root: FrameLayout
    private lateinit var keyboard: KeyboardView
    private lateinit var candidateRow: LinearLayout
    private lateinit var englishRow: LinearLayout
    private lateinit var candidateScroll: HorizontalScrollView
    private lateinit var englishScroll: HorizontalScrollView

    /** 文件 IO / 持久化（初始化、收藏、置顶、落盘），不占 CPU 线程 */
    private val io = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    /** CPU 密集的候选查询（analyze / more / t9），与文件 IO 互不拖累 */
    private val cpu = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var lookupJob: Job? = null

    private val pinyin = StringBuilder()
    private val digitRun = StringBuilder()
    /** 九键缓冲（数字串，可混入长按插入的字母） */
    private val t9buf = StringBuilder()
    private var t9Mode = false
    private var engineReady = false
    private var englishMode = false
    private var composingInfo: String = ""
    private var pageStart = 0
    private var allCandidates: List<String> = emptyList()
    private var match: TypesakeCore.Match? = null
    private var candidates: List<String> = emptyList()
    private var predictions: List<String> = emptyList()
    private var englishChips: List<TypesakeCore.EnglishOption> = emptyList()
    private var lastCommittedChinese: String = ""
    private var phrases: List<Triple<String, String, Long>> = emptyList()

    private var shifted = false
    private var capsLock = false
    private var lastShiftTap = 0L
    private var lastSpaceTap = 0L
    private var selfEdit = false

    private var kind = KbKind.QWERTY
    private var layer = KbLayer.LETTERS
    private var privateField = false

    /** 当前编辑框所属应用包名（「按应用不给英文候选」用） */
    private var currentPkg: String? = null

    /** 快捷输入缓冲：`v1+2` / `i123` / `im123.45` / `u4e00`；空 = 不在快捷输入中 */
    private val scBuf = StringBuilder()

    /** 快捷输入的分类标签（算式/中文数字/金额/字符），空 = 还没算出来 */
    private var scKind = ""

    /** 数字键直出译词：开着时按数字键上屏的是英文释义而不是中文候选 */
    private var translateOut = false

    /** 英文模式正在拼的词（拼写纠正需要一个「未定稿」的区域才能给候选） */
    private val enBuf = StringBuilder()

    /** 上面这个词的纠正结果；空 = 没打错 / 不该改 */
    private var enFix = ""

    private var colors: KbColors = KbThemes.resolve(0, 0, false)

    private val clipItems = ArrayDeque<String>()
    private var clipRegistered = false
    private val clipListener = ClipboardManager.OnPrimaryClipChangedListener { captureClipboard() }
    /** B7：设置改完立即生效（无需重新聚焦输入框） */
    private val prefsListener =
        android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, _ ->
            if (::keyboard.isInitialized) {
                colors = resolveColors()
                keyboard.render(
                    kind, layer, colors, prefs.keyHeightDp, clipItems.toList(),
                    pairList(), prefs.customSymbols, englishMode, prefs.hideNumberRow,
                )
                refreshBars()
            }
        }

    private var actionPopup: PopupWindow? = null
    private var popupPinyin: String = ""
    private var popupWord: String = ""

    // ---------------- 生命周期 ----------------

    override fun onCreate() {
        super.onCreate()
        prefs = TypesakePrefs(this)
        prefs.registerListener(prefsListener)
        // C1：词典拷贝/JSON 载入/L0 导入全部移出主线程（首次约 50–150ms）
        io.launch {
            TypesakeAssets.ensureEnglishDict(this@TypesakeImeService)
            TypesakeCore.init(filesDir.absolutePath)
            TypesakeCore.setOptions(prefs.fuzzy, prefs.correction, prefs.shuangpin)
            withContext(Dispatchers.Main) {
                engineReady = true
                if (::keyboard.isInitialized) {
                    keyboard.render(kind, layer, colors, prefs.keyHeightDp, clipItems.toList(), pairList())
                    refreshBars()
                }
            }
        }
    }

    override fun onDestroy() {
        prefs.unregisterListener(prefsListener)
        unregisterClipboard()
        io.cancel()
        cpu.cancel()
        super.onDestroy()
    }

    override fun onEvaluateFullscreenMode(): Boolean = false

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        colors = resolveColors()
        applyGlassBlur()
        if (::keyboard.isInitialized) {
            keyboard.render(
                kind, layer, colors, prefs.keyHeightDp, clipItems.toList(),
                pairList(), prefs.customSymbols, englishMode, prefs.hideNumberRow,
            )
            refreshBars()
        }
    }

    /**
     * 玻璃皮肤：API 31+ 打开系统级背景模糊（跨窗口模糊未开启时系统会忽略，不会报错）。
     * 低版本退化为半透明（alpha）玻璃观感。
     */
    private fun applyGlassBlur() {
        val dialog = window ?: return
        val attrs = dialog.window?.attributes ?: return
        if (colors.glass) {
            if (android.os.Build.VERSION.SDK_INT >= 31) {
                attrs.flags = attrs.flags or android.view.WindowManager.LayoutParams.FLAG_BLUR_BEHIND
                attrs.blurBehindRadius = dp(20)
            }
        } else {
            if (android.os.Build.VERSION.SDK_INT >= 31) {
                attrs.flags = attrs.flags and android.view.WindowManager.LayoutParams.FLAG_BLUR_BEHIND.inv()
                attrs.blurBehindRadius = 0
            }
        }
        dialog.window?.attributes = attrs
    }

    override fun onStartInput(info: EditorInfo?, restarting: Boolean) {
        super.onStartInput(info, restarting)
        val inputType = info?.inputType ?: 0
        englishMode = KbLayouts.prefersEnglish(inputType)
        kind = KbLayouts.kindForInputType(inputType)
        privateField = KbLayouts.learningDisabled(inputType)
        currentPkg = info?.packageName
        if (privateField) clearComposing()
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        if (engineReady) {
            TypesakeCore.setOptions(prefs.fuzzy, prefs.correction, prefs.shuangpin)
        }
        val startInputType = info?.inputType ?: currentInputEditorInfo?.inputType ?: 0
        englishMode = if (KbLayouts.learningDisabled(startInputType)) {
            false
        } else {
            KbLayouts.prefersEnglish(startInputType) || prefs.englishMode
        }
        t9Mode = prefs.t9Layout && (kind == KbKind.QWERTY || kind == KbKind.RAW)
        if (t9Mode) kind = KbKind.T9
        clearComposing()
        t9buf.setLength(0)
        digitRun.setLength(0)
        predictions = emptyList()
        englishChips = emptyList()
        shifted = false
        capsLock = false
        colors = resolveColors()
        phrases = loadPhrases()
        if (::keyboard.isInitialized) {
            pageStart = 0
        allCandidates = emptyList()
        keyboard.setOneHand(OneHand.fromInt(prefs.oneHand))
            keyboard.render(
                kind, layer, colors, prefs.keyHeightDp, clipItems.toList(),
                pairList(), prefs.customSymbols, englishMode, prefs.hideNumberRow,
            )
            keyboard.setShift(false, false)
            refreshBars()
        }
        registerClipboard()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        unregisterClipboard()
        lookupJob?.cancel()
        dismissActionPopup()
        if (::keyboard.isInitialized) keyboard.dismissAllPopups()
        super.onFinishInputView(finishingInput)
    }

    override fun onWindowHidden() {
        if (::keyboard.isInitialized) keyboard.dismissAllPopups()
        super.onWindowHidden()
    }

    override fun onFinishInput() {
        clearComposing()
        if (::keyboard.isInitialized) keyboard.setClipboardItems(clipItems.toList())
        // 统计计数平时只在内存（合并写入），收尾时统一落盘
        io.launch { TypesakeCore.flush() }
        super.onFinishInput()
    }

    override fun onCreateInputView(): View {
        root = FrameLayout(this)
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }

        // 中文候选在上
        candidateScroll = HorizontalScrollView(this)
        candidateRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        candidateScroll.addView(candidateRow)
        column.addView(
            candidateScroll,
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(CAND_BAR_DP))
        )

        // 英文伴学在下
        englishScroll = HorizontalScrollView(this)
        englishRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        englishScroll.addView(englishRow)
        column.addView(
            englishScroll,
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(EN_BAR_DP))
        )

        val keyboardHost = FrameLayout(this)
        keyboard = KeyboardView(this, prefs, this).apply {
            layoutParams = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
        }
        keyboardHost.addView(keyboard)
        column.addView(
            keyboardHost,
            LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
        )

        root.addView(column)
        colors = resolveColors()
        renderCandidates()
        renderEnglish()
        return root
    }

    // ---------------- 状态与渲染 ----------------

    private fun resolveColors(): KbColors =
        KbThemes.resolve(prefs.palette, prefs.themeMode, KbThemes.isNight(resources.configuration.uiMode))

    private fun pairList(): List<Pair<String, String>> =
        phrases.map { it.first to it.second }

    private fun loadPhrases(): List<Triple<String, String, Long>> =
        TypesakeCore.list().map { Triple(it.chinese, it.english, it.saved_at) }

    private fun clearComposing() {
        pinyin.setLength(0)
        t9buf.setLength(0)
        scBuf.setLength(0)
        scKind = ""
        enBuf.setLength(0)
        enFix = ""
        candidates = emptyList()
        match = null
        lastCommittedChinese = ""
        pageStart = 0
        allCandidates = emptyList()
    }

    private fun refreshBars() {
        renderCandidates()
        renderEnglish()
    }

    private fun renderCandidates() {
        if (!::candidateRow.isInitialized) return
        candidateRow.removeAllViews()
        applyBarBackground(candidateScroll)
        composingInfo = ""   // 每帧重置，避免纠错提示残留
        val vertical = prefs.verticalCandidates
        candidateRow.orientation = if (vertical) LinearLayout.VERTICAL else LinearLayout.HORIZONTAL

        // 0) 九键输入中
        if (t9Mode && t9buf.isNotEmpty()) {
            candidateRow.addView(textLabel(t9buf.toString(), colors.accent, colors.barBg))
            val list = match?.candidates ?: candidates
            if (list.isEmpty()) {
                candidateRow.addView(textLabel("继续输入数字，或长按数字键插入原字", colors.hint, colors.barBg))
            } else {
                for ((i, word) in list.withIndex()) {
                    candidateRow.addView(
                        candidateChip(
                            if (i < 9 && prefs.showCandidateIndex) "${i + 1} $word" else word,
                            word,
                        )
                    )
                }
            }
            return
        }

        // 0.5) 快捷输入中（v 算式 / i 中文数字 / im 金额 / u 码点）
        if (scBuf.isNotEmpty()) {
            val hit = TypesakeCore.shortcutOf(scBuf.toString())
            candidateRow.addView(textLabel(scBuf.toString(), colors.accent, colors.barBg))
            if (hit == null) {
                candidateRow.addView(
                    textLabel(
                        if (scKind.isEmpty()) "继续输入符号/数字，空格上屏结果" else scKind,
                        colors.hint,
                        colors.barBg,
                    )
                )
            } else {
                candidateRow.addView(
                    chip(
                        "→ ${hit.text}",
                        colors.accentText,
                        colors.accent,
                        onClick = { commitShortcut() },
                    )
                )
                candidateRow.addView(chip(hit.kind, colors.hint, colors.key))
            }
            return
        }

        // 1) 拼音输入中
        if (pinyin.isNotEmpty()) {
            val m = match
            // 拼音/纠错信息不再占用候选位：移到下方英文条左侧（见 renderEnglish）
            composingInfo = when {
                m != null && m.remembered -> "已记住 $pinyin⇒${m.matched}"
                m != null && m.corrected -> "已纠错 $pinyin→${m.matched}"
                else -> ""
            }
            val full = m?.candidates ?: candidates
            val paged = prefs.pageKeys == 1
            val perPage = if (vertical) 4 else 8
            if (paged) {
                allCandidates = if (allCandidates.isEmpty() || pageStart == 0) full else allCandidates
            }
            val list = if (paged) {
                allCandidates.drop(pageStart).take(perPage)
            } else {
                full.take(if (vertical) 4 else 8)
            }
            if (list.isEmpty()) {
                val hint = if (!TypesakeCore.available) {
                    "引擎未加载（演示模式）：请安装带 .so 的正式包"
                } else if (!engineReady) {
                    "引擎加载中…"
                } else {
                    "无候选：空格直接上屏（异常可到设置页跑「引擎自检」）"
                }
                candidateRow.addView(textLabel(hint, colors.hint, colors.barBg))
            } else {
                // 候选旁逐词译词：一次 JNI 取回整页，查不到释义的位置是 null
                val glosses =
                    if (prefs.gloss) TypesakeCore.glossLines(list) else list.map { null }
                for ((i, word) in list.withIndex()) {
                    candidateRow.addView(
                        candidateChip(
                            label = if (i < 9 && !paged && prefs.showCandidateIndex) "${i + 1} $word" else word,
                            word = word,
                            gloss = glosses.getOrNull(i),
                        )
                    )
                }
                if (prefs.gloss) {
                    // 数字键直出译词：开着时按数字键上屏的是这条候选的英文释义
                    candidateRow.addView(
                        chip(
                            if (translateOut) "译●" else "译",
                            if (translateOut) colors.accentText else colors.hint,
                            if (translateOut) colors.accent else colors.key,
                        ) {
                            translateOut = !translateOut
                            renderCandidates()
                        }
                    )
                }
            }
            if (paged && allCandidates.size > pageStart + perPage) {
                candidateRow.addView(chip("下页 ▸", colors.hint, colors.key) { flipPage(1) })
            }
            if (paged && pageStart > 0) {
                candidateRow.addView(chip("◂ 上页", colors.hint, colors.key) { flipPage(-1) })
            }
            // A3 误纠错一键还原：把原样输入也作为候选
            if (m != null && m.corrected) {
                val raw = pinyin.toString()
                if (raw.isNotEmpty() && !list.contains(raw)) {
                    candidateRow.addView(chip("原样 $raw", colors.hint, colors.key) { commitRaw(raw) })
                }
            }
            // #5 超长保护：拼音串过长时提示可直接回车上屏（不阻塞候选）
            if (pinyin.length > 40) {
                candidateRow.addView(
                    chip("过长 · 回车直接上屏", colors.accentText, colors.accent) { commitRaw(pinyin.toString()) }
                )
            }
            // B5：候选多于可见数时始终给"更多"入口（提升可发现性）
            val perPageNow = if (vertical) 4 else 8
            if (paged || full.size > perPageNow || list.size >= perPageNow) {
                candidateRow.addView(chip("更多 ▸", colors.hint, colors.key) { showMoreCandidates() })
            }
            return
        }

        // 2) 数字智能候选（手机号/日期/时间）
        if (digitRun.isNotEmpty()) {
            val decorated = TextUtils.digitCandidates(digitRun.toString())
            if (decorated.isNotEmpty()) {
                candidateRow.addView(textLabel("数字", colors.hint, colors.barBg))
                for (text in decorated) {
                    candidateRow.addView(
                        chip(text, colors.keyText, colors.key, onClick = { commitDigitDecoration(text) })
                    )
                }
                return
            }
        }

        // 3) 联想
        if (predictions.isNotEmpty()) {
            candidateRow.addView(textLabel("联想", colors.hint, colors.barBg))
            for (word in predictions) {
                candidateRow.addView(
                    chip(word, colors.barText, colors.key, onClick = { insertPrediction(word) })
                )
            }
            return
        }

        val before = currentInputConnection?.getTextBeforeCursor(32, 0)?.toString().orEmpty()
        var shown = false
        TextUtils.mixedDigitSuggestion(before)?.let { (text, replaceLen) ->
            shown = true
            candidateRow.addView(textLabel("数字识别", colors.hint, colors.barBg))
            candidateRow.addView(chip("→ $text", colors.keyText, colors.key) {
                replaceTail(replaceLen, text)
            })
        }
        for ((domain, suffix) in TextUtils.emailDomainCandidates(before, TypesakeCore.learnedMailDomains())) {
            shown = true
            candidateRow.addView(chip("$domain", colors.keyText, colors.key) {
                TypesakeCore.recallMailDomain(domain)
                commitRaw(suffix)
            })
        }
        for ((suffix, _) in TextUtils.domainSuffixCandidates(before)) {
            shown = true
            candidateRow.addView(chip(".$suffix", colors.keyText, colors.key) { commitRaw(suffix) })
        }
        if (shown) return

        candidateRow.addView(
            textLabel(
                if (privateField) "密码/隐私输入模式" else "键入拼音，如 nihao；空格上屏，双击空格收藏",
                colors.hint,
                colors.barBg,
            )
        )
    }

    /** 翻页（逗号句号翻页模式） */
    private fun flipPage(delta: Int) {
        val size = allCandidates.size
        if (size == 0) return
        val step = if (prefs.verticalCandidates) 4 else 8
        pageStart = (pageStart + delta * step).coerceIn(0, ((size - 1) / step) * step)
        renderCandidates()
    }

    /** 替换光标前 N 个字符为 text（数字识别上屏用）。 */
    private fun replaceTail(len: Int, text: String) {
        val ic = currentInputConnection ?: return
        if (len > 0) ic.deleteSurroundingText(len, 0)
        ic.commitText(text, 1)
        digitRun.setLength(0)
        refreshBars()
    }

    private fun renderEnglish() {
        if (!::englishRow.isInitialized) return
        englishRow.removeAllViews()
        applyBarBackground(englishScroll)

        // 按应用不给英文候选：屏蔽名单里的应用只留操作按钮，不注入任何英文
        if (prefs.englishBlockedFor(currentPkg)) {
            englishRow.addView(chip("已隐藏此应用的英文候选", colors.hint, colors.barBg))
            if (!privateField) {
                englishRow.addView(chip("★", colors.accentText, colors.accent, onClick = { saveCurrentSentence() }))
            }
            englishRow.addView(chip("学", colors.accentText, colors.accent, onClick = { onOpenHub() }))
            return
        }
        // 英文模式：当前正在拼的词（纠正候选就挂在这里，空格上屏时才生效）
        if (englishMode && enBuf.isNotEmpty()) {
            englishRow.addView(chip(enBuf.toString(), colors.accent, colors.barBg))
            if (enFix.isNotEmpty()) {
                englishRow.addView(
                    chip(
                        "→$enFix",
                        colors.accentText,
                        colors.key,
                        onClick = { acceptEnglishFix() },
                    )
                )
            } else {
                englishRow.addView(chip("没打错", colors.hint, colors.barBg))
            }
            if (!privateField) {
                englishRow.addView(chip("★", colors.accentText, colors.accent, onClick = { saveCurrentSentence() }))
            }
            englishRow.addView(chip("学", colors.accentText, colors.accent, onClick = { onOpenHub() }))
            return
        }
        if (composingInfo.isNotEmpty()) {
            englishRow.addView(chip(composingInfo, colors.accent, colors.barBg))
        }
        if (englishChips.isEmpty()) {
            englishRow.addView(
                chip(
                    if (privateField) "隐私输入：不转换、不学习、不收藏" else "英文表达显示在这里",
                    colors.hint,
                    colors.barBg,
                )
            )
        } else {
            for (option in englishChips) {
                val label = if (option.kindLabel.isEmpty()) {
                    option.text
                } else {
                    "${option.kindLabel}｜${option.text}"
                }
                englishRow.addView(
                    chip(
                        label,
                        colors.barText,
                        colors.key,
                        onClick = { insertEnglish(option.text) },
                        onLongClick = { replaceLastChineseWith(option.text) },
                    )
                )
            }
        }
        if (!privateField) {
            englishRow.addView(chip("★", colors.accentText, colors.accent, onClick = { saveCurrentSentence() }))
        }
        englishRow.addView(chip("学", colors.accentText, colors.accent, onClick = { onOpenHub() }))
    }

    /**
     * 英文模式：重算当前词的纠正结果。
     *
     * 纠正给得非常克制——`english_fix` 关着、词太短/不是词、或词表里没有更接近的词，
     * 一律返回空串（界面上就只显示原词，不会凭空冒出一个"修正"）。
     * 首字母大小写跟原词走，否则 `Hello` 会被改成 `hello`。
     */
    private fun updateEnglishFix() {
        val raw = enBuf.toString()
        if (!prefs.englishFix || raw.isEmpty()) {
            enFix = ""
            return
        }
        val fixed = TypesakeCore.spellingFix(raw)
        enFix = when {
            fixed.isEmpty() -> ""
            raw.first().isUpperCase() -> fixed.replaceFirstChar { it.uppercaseChar() }
            else -> fixed
        }
    }

    /** 把英文模式当前词上屏（`preferred` 指定用哪个词，空则用纠正结果，再没有就原样）。 */
    private fun commitEnglishWord(preferred: String? = null) {
        val raw = enBuf.toString()
        if (raw.isEmpty()) return
        val word = preferred?.takeIf { it.isNotEmpty() } ?: enFix.ifEmpty { raw }
        enBuf.setLength(0)
        enFix = ""
        val ic = currentInputConnection
        ic?.setComposingText("", 0)
        ic?.commitText(word, 1)
        selfEdit = true
        digitRun.setLength(0)
        renderEnglish()
    }

    /** 点了「→ 修正」：直接按纠正后的词上屏。 */
    private fun acceptEnglishFix() {
        if (enFix.isEmpty()) return
        commitEnglishWord(enFix)
    }

    /** 中文候选：字号更大（主视觉），支持长按置顶/删词。 */
    private fun candidateChip(
        label: String,
        word: String,
        gloss: TypesakeCore.GlossLine? = null,
    ): TextView {
        // 注意：GradientDrawable 自己也有 colors 属性，apply 里必须显式取外层字段
        val keyColor = colors.key
        val textColor = colors.keyText
        val accent = colors.accent
        val hint = colors.hint
        return TextView(this).apply {
            text = candidateLabel(label, gloss, accent, hint)
            setTextColor(textColor)
            textSize = 16f
            maxLines = 1
            gravity = Gravity.CENTER
            setPadding(dp(14), dp(9), dp(14), dp(9))
            background = GradientDrawable().apply {
                setColor(keyColor)
                cornerRadius = dp(9).toFloat()
            }
            isClickable = true
            setOnClickListener { commitCandidate(word) }
            setOnLongClickListener { showCandidateActions(word); true }
            layoutParams = chipParams()
        }
    }

    /**
     * 候选文案：`1 今天  ●  today`。
     * 生词加一个橙点并把译词染成主题强调色，其余译词用弱化色的小字。
     */
    private fun candidateLabel(
        label: String,
        gloss: TypesakeCore.GlossLine?,
        accent: Int,
        hint: Int,
    ): CharSequence {
        if (gloss == null || gloss.text.isEmpty()) return label
        val sb = SpannableStringBuilder(label)
        val spanFlags = Spanned.SPAN_EXCLUSIVE_EXCLUSIVE
        if (gloss.fresh) {
            val start = sb.length
            sb.append(" ●")
            sb.setSpan(RelativeSizeSpan(0.5f), start, sb.length, spanFlags)
            sb.setSpan(ForegroundColorSpan(accent), start, sb.length, spanFlags)
        }
        val text = if (gloss.text.length > 20) gloss.text.take(19) + "…" else gloss.text
        val body = if (gloss.level.isEmpty()) text else "$text ${gloss.level}"
        val from = sb.length
        sb.append("  ")
        sb.append(body)
        sb.setSpan(RelativeSizeSpan(0.62f), from, sb.length, spanFlags)
        sb.setSpan(
            ForegroundColorSpan(if (gloss.fresh) accent else hint),
            from,
            sb.length,
            spanFlags,
        )
        return sb
    }

    /** 英文/操作条：字号更小（次要信息）。 */
    private fun chip(
        label: String,
        textColor: Int,
        bg: Int,
        onClick: (() -> Unit)? = null,
        onLongClick: (() -> Unit)? = null,
    ): TextView = TextView(this).apply {
        text = label
        setTextColor(textColor)
        textSize = 13f
        maxLines = 1
        gravity = Gravity.CENTER
        setPadding(dp(9), dp(4), dp(9), dp(4))
        background = GradientDrawable().apply {
            setColor(bg)
            cornerRadius = dp(8).toFloat()
        }
        if (onClick != null) {
            isClickable = true
            setOnClickListener { onClick() }
        }
        if (onLongClick != null) {
            setOnLongClickListener {
                onLongClick()
                true
            }
        }
        layoutParams = chipParams()
    }

    /** 玻璃皮肤下面板用圆角+描边；普通皮肤仍用纯色。 */
    private fun applyBarBackground(view: android.view.View) {
        // 注意：GradientDrawable 自身也有 colors 属性，apply 里必须用提前取好的局部变量
        val barBg = colors.barBg
        val border = colors.glassBorder
        if (colors.glass) {
            view.background = GradientDrawable().apply {
                setColor(barBg)
                cornerRadius = dp(14).toFloat()
                setStroke(dp(1), border)
            }
        } else {
            view.setBackgroundColor(barBg)
        }
    }

    private fun chipParams(): LinearLayout.LayoutParams = LinearLayout.LayoutParams(
        ViewGroup.LayoutParams.WRAP_CONTENT,
        ViewGroup.LayoutParams.WRAP_CONTENT,
    ).apply {
        marginStart = dp(6)
        topMargin = dp(4)
        bottomMargin = dp(4)
    }

    private fun textLabel(label: String, color: Int, bg: Int): TextView = TextView(this).apply {
        text = label
        setTextColor(color)
        textSize = 14f
        gravity = Gravity.CENTER_VERTICAL
        setPadding(dp(8), dp(5), dp(4), dp(5))
        setBackgroundColor(bg)
    }

    // ---------------- 候选操作（长按） ----------------

    private fun showCandidateActions(word: String) {
        dismissActionPopup()
        popupPinyin = pinyin.toString()
        popupWord = word
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            setBackgroundColor(colors.bg)
            setPadding(dp(6), dp(6), dp(6), dp(6))
        }
        // A6 以词定字：多字候选可只取其中一字
        if (word.length > 1) {
            for (ch in word.take(6)) {
                row.addView(
                    chip(ch.toString(), colors.keyText, colors.key) {
                        dismissActionPopup()
                        commitRaw(ch.toString())
                    }
                )
            }
        }
        // 上屏译词：不打拼音选词，直接把该候选的英文释义上屏
        run {
            val en = TypesakeCore.shortGlossOf(word)
            if (en.isNotEmpty()) {
                row.addView(
                    chip("译词", colors.accentText, colors.accent, onClick = {
                        dismissActionPopup()
                        commitCandidateText(en)
                    })
                )
            }
        }
        // 逐词译词明细：这条候选切成了哪些词、各是什么意思
        run {
            val detail = TypesakeCore.glossDetail(word)
            if (detail.isNotEmpty()) {
                row.addView(
                    chip("明细", colors.keyText, colors.key, onClick = {
                        dismissActionPopup()
                        showGlossDetail(word, detail)
                    })
                )
            }
        }
        row.addView(
            chip("置顶", colors.accentText, colors.accent, onClick = {
                val p = popupPinyin
                val w = popupWord
                val fallback = candidates
                dismissActionPopup()
                // pin 会触发落盘（fsync），移出主线程
                io.launch {
                    TypesakeCore.pin(p, w)
                    val updated = TypesakeCore.cached(p) ?: fallback
                    withContext(Dispatchers.Main) {
                        candidates = updated
                        match = match?.copy(candidates = updated)
                        renderCandidates()
                        toast("已置顶：$w")
                    }
                }
            })
        )
        row.addView(
            chip("删词", colors.keyText, colors.key, onClick = {
                val p = popupPinyin
                val w = popupWord
                dismissActionPopup()
                io.launch {
                    val updated = TypesakeCore.forget(p, w)
                    withContext(Dispatchers.Main) {
                        candidates = updated
                        match = match?.copy(candidates = updated)
                        renderCandidates()
                        toast("已删除：$w")
                    }
                }
            })
        )
        val popup = PopupWindow(row, ViewGroup.LayoutParams.WRAP_CONTENT, dp(44), true).apply {
            isOutsideTouchable = true
        }
        val anchor = candidateRow
        val loc = IntArray(2)
        anchor.getLocationInWindow(loc)
        popup.showAtLocation(anchor, Gravity.NO_GRAVITY, loc[0] + dp(8), loc[1] + dp(40))
        actionPopup = popup
    }

    private fun dismissActionPopup() {
        actionPopup?.dismiss()
        actionPopup = null
    }

    /** 逐词译词明细：这条候选切成的每一段（词 / 释义 / 词性 / 生词 / 级别）。 */
    private fun showGlossDetail(word: String, detail: List<TypesakeCore.GlossWord>) {
        dismissActionPopup()
        val box = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(colors.bg)
            setPadding(dp(12), dp(8), dp(12), dp(8))
        }
        box.addView(textLabel(word, colors.barText, colors.bg))
        for (p in detail) {
            val head = if (p.pos.isEmpty()) p.zh else "${p.zh} ${p.pos}"
            val tail = buildString {
                append(" — ")
                append(p.en)
                if (p.level.isNotEmpty()) append("  ${p.level}")
                if (p.fresh) append("  ●")
            }
            box.addView(
                textLabel(
                    head + tail,
                    if (p.fresh) colors.accent else colors.hint,
                    colors.bg,
                )
            )
        }
        box.addView(chip("关闭", colors.keyText, colors.key) { dismissActionPopup() })
        val popup =
            PopupWindow(box, ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, true)
                .apply { isOutsideTouchable = true }
        val anchor = candidateRow
        val loc = IntArray(2)
        anchor.getLocationInWindow(loc)
        popup.showAtLocation(anchor, Gravity.NO_GRAVITY, loc[0] + dp(8), loc[1] + dp(40))
        actionPopup = popup
    }

    // ---------------- 快捷输入（v 算式 / i 中文数字 / im 金额 / u 码点） ----------------

    /**
     * 这一键要不要交给快捷输入。
     *
     * 关键取舍：**只有当形状真的被「确认」时才抢**——即拼音缓冲正好是一个
     * `v` / `i` / `u` 前缀、而新来的这一键把它坐实成快捷输入的样子。
     * 这样双拼里 `v`/`i`/`u`（zh/ch/sh 的开头）照常出候选，不被快捷输入拖住；
     * 数字键如果本来有对应候选，也让位给「选词」。
     */
    private fun shortcutKey(text: String): Boolean {
        if (text.isEmpty()) return false
        if (privateField || englishMode) return false
        if (scBuf.isNotEmpty()) {
            val next = scBuf.toString() + text
            if (shortcutShapeOk(next)) {
                scBuf.append(text)
                updateShortcut()
                return true
            }
            // 形状断了：清缓冲，这一键回落到正常流程
            exitShortcut()
            return false
        }
        if (!prefs.shortcut || t9Mode || pinyin.length != 1) return false
        val ch = text[0]
        if (!entersShortcut(pinyin[0], ch)) return false
        if (ch.isDigit()) {
            val idx = if (ch == '0') 9 else ch.digitToInt() - 1
            if ((match?.candidates ?: candidates).getOrNull(idx) != null) return false
        }
        scBuf.append(pinyin).append(ch)
        pinyin.setLength(0)
        candidates = emptyList()
        match = null
        allCandidates = emptyList()
        pageStart = 0
        selfEdit = true
        currentInputConnection?.setComposingText(scBuf.toString(), 1)
        updateShortcut()
        return true
    }

    /** 第一键后缀能不能坐实快捷输入形状（`im` 单独在 onLetter 里放行）。 */
    private fun entersShortcut(prefix: Char, next: Char): Boolean = when (prefix) {
        'v' -> next.isDigit() || next == '(' || next == '-' || next == '.'
        'i' -> next.isDigit() || next == '-' || next == '.'
        'u' -> next.isDigit()
        else -> false
    }

    /** 整条缓冲是否仍然是某个快捷输入的合法前缀；不是就退出、交还给拼音。 */
    private fun shortcutShapeOk(s: String): Boolean {
        if (s.isEmpty() || s[0] !in "viu") return false
        val rest = s.substring(1)
        if (rest.isEmpty()) return true
        return when (s[0]) {
            'v' -> rest.all { it.isDigit() || it in "()+-*/.^%" }
            'u' -> rest.all { it.isLetterOrDigit() }
            else -> if (s.startsWith("im")) s.substring(2).all { it.isDigit() || it == '.' }
            else rest.all { it.isDigit() || it == '-' || it == '.' }
        }
    }

    private fun updateShortcut() {
        val hit = TypesakeCore.shortcutOf(scBuf.toString())
        scKind = hit?.kind.orEmpty()
        renderCandidates()
    }

    private fun exitShortcut() {
        if (scBuf.isEmpty()) return
        scBuf.setLength(0)
        scKind = ""
        currentInputConnection?.setComposingText("", 0)
        renderCandidates()
    }

    /** 空格/回车/点候选：把算好的结果上屏。 */
    private fun commitShortcut() {
        val hit = TypesakeCore.shortcutOf(scBuf.toString()) ?: return
        scBuf.setLength(0)
        scKind = ""
        candidates = emptyList()
        match = null
        val ic = currentInputConnection ?: return
        ic.setComposingText("", 0)
        ic.commitText(hit.text, 1)
        selfEdit = true
        digitRun.setLength(0)
        renderCandidates()
        renderEnglish()
    }

    /**
     * 大千注音：这一键该当注音键进组合区，而不是选词/翻页/标点。
     *
     * 注音击键里含 `0-9`（ㄅㄉㄓ 与调号）和 `,` `.` `;` `/` `-`（ㄝㄡㄥㄤㄦ），
     * 与选词键、翻页键、句号键撞车，只能按方案分流：
     *
     * - 数字永远是注音键（`-` 同理，因为它能独立成音「ㄦ」，丢了就没法打）；
     * - `,` `.` `;` `/` 在**组合中**是注音键；没在组合时只认主键区
     *   （主键区的 `.` 是句号键、这里当 ㄡ），符号页的 `，` `。` 仍是普通标点——
     *   否则注音模式下一个逗号都打不出来。
     */
    private fun zhuyinInput(text: String): Boolean {
        if (prefs.shuangpin != TypesakePrefs.SCHEME_ZHUYIN) return false
        if (t9Mode || englishMode || privateField) return false
        if (text.length != 1) return false
        val ch = text[0]
        if (ch.isDigit() || ch == '-') return true
        if (ch !in ",.;:/") return false
        if (pinyin.isNotEmpty()) return true
        return layer == KbLayer.LETTERS
    }

    // ---------------- 输入逻辑 ----------------

    override fun onInsert(text: String) {
        val ic = currentInputConnection ?: return
        if (privateField) {
            ic.commitText(text, 1)
            return
        }

        // 英文模式：字母直接上屏（不转拼音）
        if (englishMode && text.length == 1 && text[0].isLetter()) {
            // 只有开了拼写纠正、且这个应用没被屏蔽英文时，才需要一个「未定稿」的词区
            if (prefs.englishFix && !prefs.englishBlockedFor(currentPkg)) {
                enBuf.append(text)
                selfEdit = true
                digitRun.setLength(0)
                pageStart = 0
                allCandidates = emptyList()
                ic.setComposingText(enBuf.toString(), 1)
                updateEnglishFix()
                renderEnglish()
                return
            }
            ic.commitText(text, 1)
            digitRun.setLength(0)
            refreshBars()
            return
        }
        // 大千注音：0-9 与 `,` `.` `;` `/` `-` 都是注音键，不是选词/翻页/标点。
        // 必须排在快捷输入之前——`u`+数字 本来就是「ㄧ+注音」，会被快捷输入抢走。
        if (zhuyinInput(text)) {
            onLetter(text)
            return
        }
        // 快捷输入：形状一旦坐实就接管（与拼音互不干扰）
        if (shortcutKey(text)) return
        // 九键：数字/字母进缓冲
        if (t9Mode && text.length == 1 && text[0].isLetterOrDigit()) {
            t9buf.append(text.lowercase())
            selfEdit = true
            ic.setComposingText(t9buf.toString(), 1)
            requestCandidates(t9buf.toString())
            return
        }
        if (t9Mode && (t9buf.isNotEmpty() || pinyin.isNotEmpty())) {
            commitTopCandidate()
        }
        // 拼音输入中：数字键选候选
        if (pinyin.isNotEmpty() && text.length == 1 && text[0].isDigit()) {
            val idx = if (text == "0") 9 else text[0].digitToInt() - 1
            val list = match?.candidates ?: candidates
            // 数字键直出译词（一次性）：这条数字上屏的是该候选的英文释义
            if (translateOut) {
                list.getOrNull(idx)?.let { cand ->
                    val en = TypesakeCore.shortGlossOf(cand)
                    if (en.isNotEmpty()) {
                        commitCandidateText(en)
                        return
                    }
                }
            }
            list.getOrNull(idx)?.let {
                commitCandidate(it)
                return
            }
        }
        // 二三候选键：输入中 , 选第 2 个、. 选第 3 个
        if (pinyin.isNotEmpty() && prefs.pageKeys == 1 && (text == "," || text == ".")) {
            flipPage(if (text == ".") 1 else -1)
            return
        }
        if (pinyin.isNotEmpty() && (text == "," || text == ".")) {
            val idx = if (text == ",") 1 else 2
            (match?.candidates ?: candidates).getOrNull(idx)?.let {
                commitCandidate(it)
                return
            }
        }
        // 字母 -> 拼音
        if (text.length == 1 && text[0].isLetter()) {
            onLetter(text)
            return
        }

        // 英文模式拼写纠正：第一个非字母键（空格/数字/标点）= 词到头了，定稿上屏
        if (enBuf.isNotEmpty()) commitEnglishWord()
        if (pinyin.isNotEmpty()) commitTopCandidate()

        if (text.length == 1) {
            val ch = text[0]
            // 成对符号：自动补全并把光标放中间；若右侧已是闭合符则直接跳过
            TextUtils.insertPairFor(ch)?.let { (open, close) ->
                val after = ic.getTextAfterCursor(1, 0)?.toString().orEmpty()
                if (after.isNotEmpty() && after[0] == close[0]) {
                    moveCursor(KeyEvent.KEYCODE_DPAD_RIGHT)
                } else {
                    ic.commitText(open + close, 1)
                    moveCursor(KeyEvent.KEYCODE_DPAD_LEFT)
                }
                digitRun.setLength(0)
                renderCandidates()
                return
            }
            // 智能标点（英文模式下强制半角）
            if (englishMode && ch in ",.;:?!") {
                ic.commitText(ch.toString(), 1)
                digitRun.setLength(0)
                renderCandidates()
                return
            }
            if (ch in ",.;:?!") {
                val before = ic.getTextBeforeCursor(1, 0)?.toString()?.lastOrNull()
                ic.commitText(TextUtils.smartPunctuation(ch, before), 1)
                digitRun.setLength(0)
                renderCandidates()
                return
            }
        }

        ic.commitText(text, 1)
        if (text.length == 1 && text[0].isDigit()) {
            digitRun.append(text)
        } else {
            digitRun.setLength(0)
        }
        renderCandidates()
    }

    private fun onLetter(letterText: String) {
        val ic = currentInputConnection ?: return
        val lower = letterText.lowercase()
        // 快捷输入 `im…`（中文大写金额）：只有全拼下才认——
        // 双拼里 `i`+`m` 是正常击键，抢了会打断输入。
        if (
            lower == "m" && pinyin.toString() == "i" && prefs.shortcut &&
            prefs.shuangpin == 0 && !englishMode && !t9Mode && !privateField
        ) {
            scBuf.append("im")
            pinyin.setLength(0)
            candidates = emptyList()
            match = null
            allCandidates = emptyList()
            pageStart = 0
            selfEdit = true
            digitRun.setLength(0)
            ic.setComposingText(scBuf.toString(), 1)
            updateShortcut()
            return
        }
        pinyin.append(lower)
        selfEdit = true
        digitRun.setLength(0)
        // 输入串变了，翻页状态必须归零，否则会显示上一串的候选页
        pageStart = 0
        allCandidates = emptyList()
        ic.setComposingText(pinyin.toString(), 1)
        val cached = TypesakeCore.cached(pinyin.toString())
        candidates = cached ?: emptyList()
        match = match?.copy(candidates = candidates)
        renderCandidates()
        requestCandidates(pinyin.toString())
        if (shifted && !capsLock) {
            shifted = false
            keyboard.setShift(false, false)
        }
    }

    private fun requestCandidates(p: String) {
        lookupJob?.cancel()
        lookupJob = cpu.launch {
            delay(15)
            if (t9Mode) {
                val list = TypesakeCore.t9(p)
                val cached = TypesakeCore.cached(p)
                withContext(Dispatchers.Main) {
                    if (t9buf.toString() == p) {
                        val hit = list.ifEmpty { cached ?: emptyList() }
                        match = TypesakeCore.Match(matched = p, corrected = false, remembered = false, candidates = hit)
                        candidates = hit
                        renderCandidates()
                        renderComposingEnglish(hit)
                    }
                }
                return@launch
            }
            val m = if (prefs.pageKeys == 1) {
                val direct = TypesakeCore.analyze(p)
                direct.copy(candidates = TypesakeCore.more(p))
            } else {
                TypesakeCore.analyze(p)
            }
            withContext(Dispatchers.Main) {
                if (pinyin.toString() == p) {
                    match = m
                    candidates = m.candidates
                    renderCandidates()
                    renderComposingEnglish(m.candidates)
                }
            }
        }
    }

    private fun renderComposingEnglish(list: List<String>) {
        if (englishChips.isNotEmpty()) return
        if (prefs.englishBlockedFor(currentPkg)) return
        val target = list.firstOrNull() ?: return
        val list = TypesakeCore.englishList(target)
        if (list.isNotEmpty()) {
            englishChips = list
            renderEnglish()
        }
    }

    /** 简繁输出（设置里开"输出繁体"时生效）。 */
    private fun applyScript(text: String): String =
        if (prefs.script == 1 && text.any { it.code in 0x4E00..0x9FFF }) {
            TypesakeCore.convert(text, true)
        } else {
            text
        }

    private fun commitCandidate(word: String) {
        val ic = currentInputConnection ?: return
        val typed = if (t9Mode) t9buf.toString() else pinyin.toString()
        val corrected = match?.corrected == true || match?.remembered == true
        val out = applyScript(word)
        ic.commitText(out, 1)
        selfEdit = true
        pageStart = 0
        allCandidates = emptyList()
        pinyin.setLength(0)
        t9buf.setLength(0)
        scBuf.setLength(0)
        scKind = ""
        candidates = emptyList()
        match = null
        lastCommittedChinese = word
        if (typed.isNotEmpty() && !t9Mode) {
            io.launch { TypesakeCore.pick(typed, word, corrected) }
        }
        TypesakeCore.context(word)
        afterCommit(word)
    }

    /**
     * 上屏一段任意文本（译词直出）：状态清理与 [`commitCandidate`] 一致，
     * 但**不学习、不换上下文**——上屏的是英文，不该进中文的 bigram。
     */
    private fun commitCandidateText(text: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(text, 1)
        selfEdit = true
        pageStart = 0
        allCandidates = emptyList()
        pinyin.setLength(0)
        t9buf.setLength(0)
        scBuf.setLength(0)
        scKind = ""
        candidates = emptyList()
        match = null
        digitRun.setLength(0)
        translateOut = false
        renderCandidates()
    }

    /** 原样上屏（不做纠错/不学习），并清空组合状态。 */
    private fun commitRaw(text: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(applyScript(text), 1)
        selfEdit = true
        pinyin.setLength(0)
        t9buf.setLength(0)
        candidates = emptyList()
        match = null
        refreshBars()
    }

    /** A4：候选翻页（最多 24 条） */
    private fun showMoreCandidates() {
        val typed = if (t9Mode) t9buf.toString() else pinyin.toString()
        if (typed.isEmpty()) return
        cpu.launch {
            val list = TypesakeCore.more(typed)
            withContext(Dispatchers.Main) { showListPopup(list) }
        }
    }

    private fun showListPopup(items: List<String>) {
        if (items.isEmpty()) {
            toast("没有更多候选")
            return
        }
        dismissActionPopup()
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val scroll = android.widget.ScrollView(this)
        for (w in items) {
            column.addView(
                chip(w, colors.keyText, colors.key) {
                    dismissActionPopup()
                    commitRaw(w)
                },
                LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT,
                ),
            )
        }
        scroll.addView(column)
        val popup = PopupWindow(scroll, dp(240), dp(260), true).apply { isOutsideTouchable = true }
        val loc = IntArray(2)
        candidateRow.getLocationInWindow(loc)
        popup.showAtLocation(candidateRow, Gravity.NO_GRAVITY, loc[0] + dp(8), loc[1] + dp(44))
        actionPopup = popup
    }

    private fun commitTopCandidate() {
        if (scBuf.isNotEmpty()) {
            commitShortcut()
            return
        }
        val top = (match?.candidates ?: candidates).firstOrNull() ?: pinyin.toString()
        if (pinyin.isNotEmpty()) commitCandidate(top)
    }

    private fun afterCommit(word: String) {
        englishChips = TypesakeCore.englishList(word)
        predictions = TypesakeCore.predict(word)
        refreshBars()
        io.launch { TypesakeCore.bump(LocalDate.now().toString()) }
    }

    private fun commitDigitDecoration(text: String) {
        val ic = currentInputConnection ?: return
        if (digitRun.isNotEmpty()) {
            ic.deleteSurroundingText(digitRun.length, 0)
        }
        ic.commitText(text, 1)
        digitRun.setLength(0)
        renderCandidates()
    }

    private fun insertPrediction(word: String) {
        val ic = currentInputConnection ?: return
        ic.commitText(applyScript(word), 1)
        lastCommittedChinese = word
        TypesakeCore.context(word)
        englishChips = TypesakeCore.englishList(word)
        predictions = TypesakeCore.predict(word)
        refreshBars()
        io.launch { TypesakeCore.bump(LocalDate.now().toString()) }
    }

    private fun insertEnglish(english: String) {
        val ic = currentInputConnection ?: return
        if (enBuf.isNotEmpty()) commitEnglishWord()
        if (pinyin.isNotEmpty()) commitTopCandidate()
        ic.commitText(if (prefs.englishAutoSpace) "$english " else english, 1)
        digitRun.setLength(0)
        predictions = emptyList()
        englishChips = TypesakeCore.englishList(lastCommittedChinese)
        refreshBars()
    }

    /** 长按英文：把刚上屏的中文替换成英文（中英混输的"改写"用法）。 */
    private fun replaceLastChineseWith(english: String) {
        val ic = currentInputConnection ?: return
        val cn = lastCommittedChinese
        if (cn.isNotEmpty() && cn != english) {
            ic.deleteSurroundingText(cn.length, 0)
            ic.commitText(english, 1)
            lastCommittedChinese = english
            toast("已改写为：$english")
        } else {
            ic.commitText(english, 1)
        }
    }

    override fun onSpace() {
        val ic = currentInputConnection ?: return
        // 英文模式拼写纠正：空格 = 词到头了定稿（开着纠正时上屏的是纠正后的词）
        if (englishMode && enBuf.isNotEmpty()) {
            commitEnglishWord()
            ic.commitText(" ", 1)
            lastSpaceTap = 0L
            return
        }
        if (t9Mode && t9buf.isNotEmpty()) {
            commitTopCandidate()
            return
        }
        if (pinyin.isNotEmpty()) {
            commitTopCandidate()
        } else {
            ic.commitText(" ", 1)
            digitRun.setLength(0)
        }
        if (englishMode) {
            lastSpaceTap = 0L
            return
        }
        val now = SystemClock.uptimeMillis()
        if (now - lastSpaceTap < DOUBLE_TAP_MS) {
            lastSpaceTap = 0L
            saveCurrentSentence()
        } else {
            lastSpaceTap = now
        }
    }

    override fun onEnter() {
        val ic = currentInputConnection ?: return
        // 快捷输入：回车 = 上屏算好的结果
        if (scBuf.isNotEmpty()) {
            commitShortcut()
            return
        }
        // 英文模式拼写纠正：回车 = 把当前词上屏（带纠正）
        if (englishMode && enBuf.isNotEmpty()) {
            commitEnglishWord()
            return
        }
        // 学商用输入法：回车 = 把输入串按英文原样上屏（不选中文候选）
        if (t9Mode && t9buf.isNotEmpty()) {
            commitRaw(t9buf.toString())
            return
        }
        if (pinyin.isNotEmpty()) {
            commitRaw(pinyin.toString())
            return
        }
        val info = currentInputEditorInfo
        val noEnterAction = info != null && (info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0
        val multiline = info != null && (info.inputType and InputType.TYPE_TEXT_FLAG_MULTI_LINE) != 0
        val action = info?.imeOptions?.and(EditorInfo.IME_MASK_ACTION) ?: EditorInfo.IME_ACTION_NONE
        if (noEnterAction || multiline || action == EditorInfo.IME_ACTION_NONE) {
            ic.commitText("\n", 1)
        } else {
            ic.performEditorAction(action)
        }
    }

    override fun onBackspace() {
        val ic = currentInputConnection ?: return
        // 快捷输入：退格先削快捷缓冲
        if (scBuf.isNotEmpty()) {
            scBuf.deleteCharAt(scBuf.length - 1)
            selfEdit = true
            if (scBuf.isEmpty()) {
                scKind = ""
                ic.setComposingText("", 0)
                candidates = emptyList()
                match = null
                renderCandidates()
            } else {
                ic.setComposingText(scBuf.toString(), 1)
                updateShortcut()
            }
            return
        }
        // 英文模式拼写纠正：退格先削当前词
        if (englishMode && enBuf.isNotEmpty()) {
            enBuf.deleteCharAt(enBuf.length - 1)
            selfEdit = true
            if (enBuf.isEmpty()) {
                enFix = ""
                ic.setComposingText("", 0)
            } else {
                ic.setComposingText(enBuf.toString(), 1)
                updateEnglishFix()
            }
            renderEnglish()
            return
        }
        if (t9Mode && t9buf.isNotEmpty()) {
            t9buf.deleteCharAt(t9buf.length - 1)
            selfEdit = true
            if (t9buf.isEmpty()) {
                ic.setComposingText("", 1)
                candidates = emptyList()
                match = null
                renderCandidates()
            } else {
                ic.setComposingText(t9buf.toString(), 1)
                requestCandidates(t9buf.toString())
            }
            return
        }
        if (pinyin.isNotEmpty()) {
            pinyin.deleteCharAt(pinyin.length - 1)
            selfEdit = true
            // 输入串变了，翻页状态归零（否则显示上一串的候选页）
            pageStart = 0
            allCandidates = emptyList()
            if (pinyin.isEmpty()) {
                ic.setComposingText("", 1)
                candidates = emptyList()
                match = null
                renderCandidates()
            } else {
                ic.setComposingText(pinyin.toString(), 1)
                val cached = TypesakeCore.cached(pinyin.toString())
                candidates = cached ?: emptyList()
                match = match?.copy(candidates = candidates)
                renderCandidates()
                requestCandidates(pinyin.toString())
            }
            return
        }
        if (digitRun.isNotEmpty()) {
            digitRun.deleteCharAt(digitRun.length - 1)
        }
        ic.deleteSurroundingText(1, 0)
        renderCandidates()
    }

    /** 滑动删除：一次删掉光标前的一个词块。 */
    override fun onDeleteWord() {
        val ic = currentInputConnection ?: return
        if (enBuf.isNotEmpty()) {
            clearComposing()
            ic.setComposingText("", 1)
            renderEnglish()
            return
        }
        if (t9Mode && t9buf.isNotEmpty()) {
            t9buf.setLength(0)
            ic.setComposingText("", 1)
            candidates = emptyList()
            match = null
            renderCandidates()
            return
        }
        if (pinyin.isNotEmpty()) {
            clearComposing()
            ic.setComposingText("", 1)
            renderCandidates()
            return
        }
        val before = ic.getTextBeforeCursor(48, 0)?.toString().orEmpty()
        val n = TextUtils.deleteWordLength(before)
        if (n > 0) ic.deleteSurroundingText(n, 0)
        digitRun.setLength(0)
        renderCandidates()
    }

    override fun onShiftTap() {
        val now = SystemClock.uptimeMillis()
        if (capsLock) {
            capsLock = false
            shifted = false
        } else if (now - lastShiftTap < DOUBLE_TAP_MS) {
            capsLock = true
            shifted = true
        } else {
            shifted = !shifted
        }
        lastShiftTap = now
        keyboard.setShift(shifted, capsLock)
    }

    override fun onShiftLongPress() {
        capsLock = true
        shifted = true
        keyboard.setShift(shifted, capsLock)
    }

    override fun onShowLayer(l: KbLayer) {
        layer = l
        if (l == KbLayer.PHRASES) phrases = loadPhrases()
        keyboard.render(kind, layer, colors, prefs.keyHeightDp, clipItems.toList(), pairList())
    }

    override fun onLanguage() {
        (getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)?.showInputMethodPicker()
    }

    override fun onOpenHub() {
        startActivity(
            Intent(this, HubActivity::class.java).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
        )
    }

    override fun onOneHandToggle() {
        val next = OneHand.fromInt(prefs.oneHand).next()
        prefs.oneHand = next.ordinal
        keyboard.setOneHand(next)
        toast(
            when (next) {
                OneHand.NONE -> "单手模式已关闭"
                OneHand.LEFT -> "单手模式：左手"
                OneHand.RIGHT -> "单手模式：右手"
            }
        )
    }

    override fun onCursorLeft() = moveCursor(KeyEvent.KEYCODE_DPAD_LEFT)

    override fun onCursorRight() = moveCursor(KeyEvent.KEYCODE_DPAD_RIGHT)

    override fun onToggleEnglish() {
        // 切回中文前，先把英文模式里没定稿的词上屏，免得丢字
        if (englishMode && enBuf.isNotEmpty()) commitEnglishWord()
        englishMode = !englishMode
        prefs.englishMode = englishMode
        if (englishMode && pinyin.isNotEmpty()) commitTopCandidate()
        toast(if (englishMode) "英文输入" else "中文输入")
        keyboard.render(
            kind, layer, colors, prefs.keyHeightDp, clipItems.toList(),
            pairList(), prefs.customSymbols, englishMode, prefs.hideNumberRow,
        )
        refreshBars()
    }

    override fun onHideKeyboard() {
        requestHideSelf(0)
    }

    override fun onOpenSettings() {
        startActivity(Intent(this, MainActivity::class.java).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) })
    }

    override fun onClipboardClear() {
        clipItems.clear()
        keyboard.setClipboardItems(emptyList())
        toast("剪贴板历史已清空")
    }

    private fun moveCursor(keyCode: Int) {
        val ic = currentInputConnection ?: return
        if (pinyin.isNotEmpty()) commitTopCandidate()
        ic.sendKeyEvent(KeyEvent(KeyEvent.ACTION_DOWN, keyCode))
        ic.sendKeyEvent(KeyEvent(KeyEvent.ACTION_UP, keyCode))
    }

    // ---------------- 收藏 ----------------

    private fun saveCurrentSentence() {
        if (privateField) {
            toast("隐私输入模式下不收藏")
            return
        }
        val ic = currentInputConnection ?: return
        val before = ic.getTextBeforeCursor(240, 0)?.toString().orEmpty()
        val sentence = TextUtils.sentenceBefore(before)
        if (sentence.isBlank()) {
            toast("先输入一句中文，再点 ★ 或双击空格收藏")
            return
        }
        // 查词 + 收藏落盘（fsync）都移出主线程，避免双击空格时卡 UI
        io.launch {
            val english = TypesakeCore.suggest(sentence)
            val ok = TypesakeCore.save(sentence, english)
            val updated = if (ok) loadPhrases() else null
            withContext(Dispatchers.Main) {
                if (updated != null) phrases = updated
                toast(
                    if (!ok) {
                        "收藏失败"
                    } else if (english.isEmpty()) {
                        "已收藏 ★ $sentence（英文待补）"
                    } else {
                        "已收藏 ★ $sentence → $english"
                    }
                )
            }
        }
    }

    // ---------------- 剪贴板 ----------------

    private fun registerClipboard() {
        if (!prefs.clipboardHistory || clipRegistered) return
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
        cm.addPrimaryClipChangedListener(clipListener)
        clipRegistered = true
        captureClipboard()
    }

    private fun unregisterClipboard() {
        if (!clipRegistered) return
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
        cm?.removePrimaryClipChangedListener(clipListener)
        clipRegistered = false
    }

    private fun captureClipboard() {
        if (!prefs.clipboardHistory) return
        val cm = getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
        val text = cm.primaryClip
            ?.takeIf { it.itemCount > 0 }
            ?.getItemAt(0)
            ?.coerceToText(this)
            ?.toString()
            ?.trim()
        if (text.isNullOrEmpty() || text.length > 1000) return
        clipItems.remove(text)
        clipItems.addFirst(text)
        while (clipItems.size > MAX_CLIP) clipItems.removeLast()
        if (::keyboard.isInitialized) keyboard.setClipboardItems(clipItems.toList())
    }

    // ---------------- 选区变化 ----------------

    override fun onUpdateSelection(
        oldSelStart: Int,
        oldSelEnd: Int,
        newSelStart: Int,
        newSelEnd: Int,
        candidatesStart: Int,
        candidatesEnd: Int,
    ) {
        super.onUpdateSelection(
            oldSelStart,
            oldSelEnd,
            newSelStart,
            newSelEnd,
            candidatesStart,
            candidatesEnd,
        )
        if (selfEdit) {
            selfEdit = false
            return
        }
        if (pinyin.isNotEmpty() && (newSelStart != oldSelStart || newSelEnd != oldSelEnd)) {
            pinyin.setLength(0)
            candidates = emptyList()
            match = null
            currentInputConnection?.finishComposingText()
            renderCandidates()
        }
    }

    // ---------------- 小工具 ----------------

    private fun toast(msg: String) {
        Toast.makeText(this, msg, Toast.LENGTH_SHORT).show()
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private companion object {
        const val DOUBLE_TAP_MS = 420L
        const val MAX_CLIP = 20
        const val CAND_BAR_DP = 44
        const val EN_BAR_DP = 36
    }
}
