package com.typesake.app.kb

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.os.Handler
import android.os.Looper
import android.util.TypedValue
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.SoundEffectConstants
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.PopupWindow
import android.widget.ScrollView
import android.widget.TextView
import com.typesake.app.TypesakePrefs

/**
 * 自绘键盘：圆角键帽、按下态、震动/声音、按键预览、长按变体、
 * 单手模式、emoji / 剪贴板面板。对外只通过 [Listener] 与输入法服务通信。
 */
class KeyboardView(
    context: Context,
    private val prefs: TypesakePrefs,
    private val listener: Listener? = null,
) : LinearLayout(context) {

    interface Listener {
        fun onInsert(text: String)
        fun onBackspace()
        fun onEnter()
        fun onSpace()
        fun onShiftTap()
        fun onShiftLongPress()
        fun onShowLayer(layer: KbLayer)
        fun onLanguage()
        fun onOpenHub()
        fun onOneHandToggle()
        fun onCursorLeft()
        fun onCursorRight()
        fun onClipboardClear()
        fun onDeleteWord()
        fun onToggleEnglish()
        fun onHideKeyboard()
        fun onOpenSettings()
    }

    var kind: KbKind = KbKind.QWERTY
        private set
    var layer: KbLayer = KbLayer.LETTERS
        private set
    var oneHand: OneHand = OneHand.NONE
        private set

    private var shifted = false
    private var capsLock = false
    private var colors: KbColors = KbThemes.resolve(KbPalette.GREEN, KbThemes.THEME_LIGHT, false)
    private var rowHeightPx: Int = dp(46)
    private var clipItems: List<String> = emptyList()
    private var clipPhrases: List<Pair<String, String>> = emptyList()
    private var customSymbols: String = ""
    private var englishMode = false
    private var hideNumberRow = false

    private val rows = LinearLayout(context).apply { orientation = VERTICAL }
    private val handler = Handler(Looper.getMainLooper())
    private var downX = 0f
    private var downY = 0f
    private var preview: PopupWindow? = null
    private var alternates: PopupWindow? = null
    private var longPress: Runnable? = null
    private var repeatRunnable: Runnable? = null

    init {
        orientation = VERTICAL
        // A1/A3：左右安全边距 + 底部手势避让（最外侧键不再贴屏幕边）
        setPadding(dp(6), 0, dp(6), dp(10))
        addView(rows, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
    }

    // ---------------- 对外 API ----------------

    /**
     * 重绘键盘。
     *
     * 参数一律不给默认值：漏传 `customSymbols` / `englishMode` / `hideNumberRow` 会静默
     * 把用户设置重置掉（之前进出符号页就会丢自定义符号）。
     */
    fun render(
        kind: KbKind,
        layer: KbLayer,
        colors: KbColors,
        keyHeightDp: Int,
        clipboardItems: List<String>,
        phrases: List<Pair<String, String>>,
        customSymbols: String,
        englishMode: Boolean,
        hideNumberRow: Boolean,
    ) {
        this.kind = kind
        this.englishMode = englishMode
        this.hideNumberRow = hideNumberRow
        this.layer = if (kind == KbKind.QWERTY || kind == KbKind.RAW) layer else KbLayer.LETTERS
        this.colors = colors
        // A4：键高上限 = 屏高 45% / 行数，防止大键高把输入区挤没
        val maxDp = (resources.displayMetrics.heightPixels / resources.displayMetrics.density * 0.45f / 6f).toInt()
        val wanted = TypesakePrefs.sanitizeKeyHeight(keyHeightDp)
        this.rowHeightPx = dp(wanted.coerceAtMost(maxDp.coerceAtLeast(TypesakePrefs.MIN_KEY_HEIGHT)))
        this.clipItems = clipboardItems
        this.clipPhrases = phrases
        this.customSymbols = customSymbols
        dismissPopups()
        rows.removeAllViews()
        if (colors.glass) {
            background = plate(colors.bg, dp(20), colors.glassBorder)
        } else {
            setBackgroundColor(colors.bg)
        }
        when (this.layer) {
            KbLayer.EMOJI -> buildEmojiPanel()
            KbLayer.CLIPBOARD -> buildClipboardPanel()
            KbLayer.PHRASES -> buildPhrasesPanel()
            else -> buildKeys()
        }
    }

    /**
     * 大小写切换：只改字母键的字，不重建整块键盘。
     *
     * 重建一次要 new ~40 个 KeyButton，每个还带两个 GradientDrawable + StateListDrawable，
     * 而大小写切换是「每按一个带 Shift 的字母都会走一次」的高频路径。
     */
    fun setShift(shifted: Boolean, capsLock: Boolean) {
        if (this.shifted == shifted && this.capsLock == capsLock) return
        this.shifted = shifted
        this.capsLock = capsLock
        if (layer != KbLayer.LETTERS) return
        val upper = shifted || capsLock
        val shiftLabel = if (capsLock) "⇪" else "⇧"
        for (i in 0 until rows.childCount) {
            val row = rows.getChildAt(i) as? ViewGroup ?: continue
            for (j in 0 until row.childCount) {
                val key = row.getChildAt(j) as? KeyButton ?: continue
                val id = key.key.id
                val lower = id.removePrefix(KEY_PREFIX_LETTER)
                when {
                    // 字母键：大小写改写（id 恒为小写，取回小写字母再按当前状态写回）
                    id.startsWith(KEY_PREFIX_LETTER) && lower.length == 1 && lower[0].isLetter() ->
                        key.setLetter(if (upper) lower.uppercaseChar() else lower.lowercaseChar())
                    // Shift 键自己也要换字面（⇧ / ⇪）
                    id == KEY_ID_SHIFT -> key.setLabel(shiftLabel)
                }
            }
        }
    }

    fun setClipboardItems(items: List<String>) {
        clipItems = items
        if (layer == KbLayer.CLIPBOARD) {
            rows.removeAllViews()
            buildClipboardPanel()
        }
    }

    fun setPhrases(phrases: List<Pair<String, String>>) {
        clipPhrases = phrases
        if (layer == KbLayer.PHRASES) {
            rows.removeAllViews()
            buildPhrasesPanel()
        }
    }

    fun setOneHand(mode: OneHand) {
        oneHand = mode
        val lp = layoutParams
        if (lp != null) {
            // B3：优先用父容器实际宽度（不含系统栏），避免右侧被裁
            val avail = (parent as? View)?.width?.takeIf { it > 0 }
                ?: resources.displayMetrics.widthPixels
            lp.width = if (mode == OneHand.NONE) {
                ViewGroup.LayoutParams.MATCH_PARENT
            } else {
                (avail * prefs.oneHandScale / 100f).toInt()
            }
            if (lp is FrameLayout.LayoutParams) {
                lp.gravity = when (mode) {
                    OneHand.NONE -> Gravity.NO_GRAVITY
                    OneHand.LEFT -> Gravity.START
                    OneHand.RIGHT -> Gravity.END
                }
            }
            layoutParams = lp
        }
    }

    override fun onDetachedFromWindow() {
        dismissPopups()
        super.onDetachedFromWindow()
    }

    // ---------------- 键盘构建 ----------------

    private fun buildKeys() {
        val layout = KbLayouts.rowsFor(kind, layer, shifted, capsLock, customSymbols, englishMode, hideNumberRow)
        val lastIndex = layout.lastIndex
        layout.forEachIndexed { index, row ->
            val rowView = LinearLayout(context).apply { orientation = HORIZONTAL }
            val h = if (index == lastIndex) rowHeightPx + dp(4) else rowHeightPx
            // A2：第二字母行左右各缩半键，形成主流输入法的错位手感
            val stagger = index == 2 && row.keys.size >= 7
            if (stagger) rowView.addView(View(context), LayoutParams(0, h, 0.5f))
            for (key in row.keys) {
                rowView.addView(KeyButton(key).apply { layoutParams = LayoutParams(0, h, key.weight) })
            }
            if (stagger) rowView.addView(View(context), LayoutParams(0, h, 0.5f))
            rows.addView(
                rowView,
                LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT).apply {
                    topMargin = dp(6)
                }
            )
        }
    }

    private fun buildEmojiPanel() {
        val scroll = ScrollView(context)
        val grid = LinearLayout(context).apply { orientation = VERTICAL }
        val perRow = 8
        for (chunk in KbLayouts.EMOJI.chunked(perRow)) {
            val rowView = LinearLayout(context).apply { orientation = HORIZONTAL }
            for (emoji in chunk) {
                rowView.addView(
                    KeyButton(
                        KbKey("emoji_$emoji", emoji, 1f, KbAction.Insert(emoji))
                    ).apply { layoutParams = LayoutParams(0, rowHeightPx, 1f) }
                )
            }
            // 补足空位，保持网格对齐
            repeat(perRow - chunk.size) {
                rowView.addView(View(context), LayoutParams(0, rowHeightPx, 1f))
            }
            grid.addView(rowView)
        }
        scroll.addView(grid)
        rows.addView(
            scroll,
            LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx * 4).apply { topMargin = dp(4) }
        )
        rows.addView(
            actionBar(
                listOf(
                    BarAction("ABC", onClick = { listener?.onShowLayer(KbLayer.LETTERS) }),
                )
            ),
            LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx).apply { topMargin = dp(4) }
        )
    }

    private fun buildClipboardPanel() {
        val scroll = ScrollView(context)
        val list = LinearLayout(context).apply { orientation = VERTICAL }
        if (clipItems.isEmpty()) {
            list.addView(
                TextView(context).apply {
                    text = "剪贴板暂无内容（仅记录本次会话，不落盘）"
                    setTextColor(colors.hint)
                    setPadding(dp(12), dp(10), dp(12), dp(10))
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                }
            )
        } else {
            for ((i, item) in clipItems.withIndex()) {
                val oneLine = item.replace('\n', ' ').take(60)
                list.addView(
                    TextView(context).apply {
                        text = oneLine
                        setTextColor(colors.keyText)
                        gravity = Gravity.CENTER_VERTICAL
                        maxLines = 1
                        setPadding(dp(12), 0, dp(12), 0)
                        background = plate(colors.key, dp(10), colors.glassBorder)
                        setOnClickListener { listener?.onInsert(item) }
                    },
                    LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx).apply {
                        bottomMargin = dp(4)
                    }
                )
                if (i == 19) break
            }
        }
        scroll.addView(list)
        rows.addView(
            scroll,
            LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx * 4).apply { topMargin = dp(4) }
        )
        rows.addView(
            actionBar(
                listOf(
                    BarAction("ABC", onClick = { listener?.onShowLayer(KbLayer.LETTERS) }),
                    BarAction("清空", onClick = { listener?.onClipboardClear() }),
                )
            ),
            LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx).apply { topMargin = dp(4) }
        )
    }

    private fun buildPhrasesPanel() {
        val scroll = ScrollView(context)
        val list = LinearLayout(context).apply { orientation = VERTICAL }
        if (clipPhrases.isEmpty()) {
            list.addView(
                TextView(context).apply {
                    text = "还没有收藏。打拼音后点 ★ 或双击空格收藏整句。"
                    setTextColor(colors.hint)
                    setPadding(dp(12), dp(10), dp(12), dp(10))
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                }
            )
        } else {
            for ((cn, en) in clipPhrases.take(20)) {
                val title = if (en.isBlank()) cn else "$cn  ·  $en"
                list.addView(
                    TextView(context).apply {
                        text = title
                        setTextColor(colors.keyText)
                        gravity = Gravity.CENTER_VERTICAL
                        maxLines = 1
                        setPadding(dp(12), 0, dp(12), 0)
                        background = plate(colors.key, dp(10), colors.glassBorder)
                        setOnClickListener { listener?.onInsert(cn) }
                        setOnLongClickListener {
                            if (en.isNotBlank()) listener?.onInsert(en)
                            true
                        }
                    },
                    LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx).apply {
                        bottomMargin = dp(4)
                    }
                )
            }
        }
        scroll.addView(list)
        rows.addView(
            scroll,
            LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx * 4).apply { topMargin = dp(4) }
        )
        rows.addView(
            actionBar(
                listOf(
                    BarAction("ABC", onClick = { listener?.onShowLayer(KbLayer.LETTERS) }),
                    BarAction("提示：长按插入英文", onClick = {}),
                )
            ),
            LayoutParams(LayoutParams.MATCH_PARENT, rowHeightPx).apply { topMargin = dp(4) }
        )
    }

    private class BarAction(val label: String, val onClick: () -> Unit)

    private fun actionBar(items: List<BarAction>): LinearLayout {
        val row = LinearLayout(context).apply { orientation = HORIZONTAL }
        for (item in items) {
            val v = TextView(context).apply {
                text = item.label
                gravity = Gravity.CENTER
                setTextColor(colors.actionText)
                background = plate(colors.actionKey, dp(10), colors.glassBorder)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                setOnClickListener { item.onClick() }
            }
            row.addView(v, LayoutParams(0, rowHeightPx, 1f).apply { marginStart = dp(4) })
        }
        return row
    }

    private fun rounded(color: Int, radius: Int): GradientDrawable =
        GradientDrawable().apply {
            setColor(color)
            cornerRadius = radius.toFloat()
        }

    /** 玻璃皮肤：键帽/面板带 1dp 描边，形成"浮起玻璃片"的观感。 */
    private fun plate(color: Int, radius: Int, borderColor: Int): GradientDrawable =
        GradientDrawable().apply {
            setColor(color)
            cornerRadius = radius.toFloat()
            if (borderColor != 0) {
                setStroke(dp(1), borderColor)
            }
        }

    private fun keyBackground(pressed: GradientDrawable): StateListDrawable = StateListDrawable().apply {
        addState(intArrayOf(android.R.attr.state_pressed), pressed)
    }

    // ---------------- 按键 ----------------

    private inner class KeyButton(var key: KbKey) : TextView(context) {

        init {
            text = swipeHint(key.label)
            gravity = Gravity.CENTER
            maxLines = 1
            isClickable = true
            setTextColor(if (key.isActionKey) colors.actionText else colors.keyText)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, if (key.isActionKey) 13f else 18f)
            val idle = plate(
                if (key.isActionKey) colors.actionKey else colors.key,
                dp(if (colors.glass) 11 else 7),
                colors.glassBorder,
            )
            val down = plate(
                if (key.isActionKey) colors.actionKey else colors.accent,
                dp(if (colors.glass) 11 else 7),
                colors.glassBorder,
            )
            background = keyBackground(down).apply {
                addState(intArrayOf(), idle)
            }
            setPadding(dp(2), 0, dp(2), 0)
            setContentDescription(key.label)
            setOnTouchListener { v, e -> handleTouch(this, e) }
        }

        /**
         * 大小写原地改写（见 [setShift]）：只换字面与插入文本，不重建键帽。
         *
         * 字母键的 id 恒为小写（`letter_a`），所以从 id 取小写字母、再按当前大小写写回。
         */
        fun setLetter(ch: Char) {
            val lower = key.id.removePrefix(KEY_PREFIX_LETTER)
            if (lower.length != 1 || !lower[0].isLetter()) return
            val label = ch.toString()
            key = key.copy(label = label, action = KbAction.Insert(label))
            text = swipeHint(label)
            contentDescription = label
        }

        /** 只换字面（Shift 键 ⇧/⇪ 这类动作键用，行为不变）。 */
        fun setLabel(label: String) {
            key = key.copy(label = label)
            text = label
            contentDescription = label
        }
    }

    /** 学百度：键面右上角用浅色小字提示"上滑符号"，解决"不知道有这功能"。 */
    private fun swipeHint(label: String): CharSequence {
        val swipeSym = KbLayouts.swipeUpFor(label)
        if (swipeSym == null || label.length != 1) return label
        val sp = android.text.SpannableString("$label $swipeSym")
        val start = sp.length - swipeSym.length
        sp.setSpan(
            android.text.style.RelativeSizeSpan(0.62f),
            start, sp.length, android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        sp.setSpan(
            android.text.style.ForegroundColorSpan(colors.hint),
            start, sp.length, android.text.Spannable.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        return sp
    }

    private fun handleTouch(v: View, e: MotionEvent): Boolean {
        when (e.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                v.isPressed = true
                downX = e.x
                downY = e.y
                feedback(v)
                if (!(v as KeyButton).key.isActionKey) showPreview(v)
                scheduleLongPress(v)
                return true
            }
            MotionEvent.ACTION_UP -> {
                cancelPendingLongPress()
                stopRepeat()
                v.isPressed = false
                dismissPreview()
                // 上滑手势天然会滑出键的上边界：滑动判定不要求仍在键内
                val key = (v as KeyButton).key
                val dx = e.x - downX
                val dy = e.y - downY
                val swiped = (dy < -dp(12) && dx < dp(18)) ||
                    (key.action is KbAction.Backspace && dx < -dp(12)) ||
                    (key.action is KbAction.Space && (dx > dp(12) || dx < -dp(12)))
                if (alternates == null && (inside(v, e) || swiped)) {
                    when {
                        // A7：阈值按密度换算（原来是像素，高 DPI 屏上过灵敏）
                        dy < -dp(12) && dx < dp(18) -> swipeUp(key)
                        key.action is KbAction.Backspace && dx < -dp(12) ->
                            listener?.onDeleteWord()
                        key.action is KbAction.Space && dx > dp(12) ->
                            listener?.onCursorRight()
                        key.action is KbAction.Space && dx < -dp(12) ->
                            listener?.onCursorLeft()
                        else -> perform(key)
                    }
                }
                dismissAlternates()
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                cancelPendingLongPress()
                stopRepeat()
                v.isPressed = false
                dismissPreview()
                dismissAlternates()
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                val dx = e.x - downX
                val dy = e.y - downY
                // 一旦判定为滑动，立即取消长按（否则长按弹层会抢走上滑手势）
                if (kotlin.math.abs(dx) > dp(12) || kotlin.math.abs(dy) > dp(12)) {
                    cancelPendingLongPress()
                }
                if (!inside(v, e)) {
                    v.isPressed = false
                    dismissPreview()
                }
            }
        }
        return false
    }

    /** 上滑：字母/符号键插入备选字符（搜狗"上滑符号"的等价实现）。 */
    private fun swipeUp(key: KbKey) {
        val alt = KbLayouts.swipeUpFor(key.label)
        if (alt != null) {
            listener?.onInsert(alt)
        } else {
            perform(key)
        }
    }

    private fun inside(v: View, e: MotionEvent): Boolean =
        e.x >= 0 && e.y >= 0 && e.x <= v.width && e.y <= v.height

    private fun feedback(v: View) {
        if (prefs.haptics) {
            when (prefs.hapticLevel) {
                0 -> Unit
                1 -> v.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                2 -> v.performHapticFeedback(HapticFeedbackConstants.VIRTUAL_KEY)
                else -> v.performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
            }
        }
        if (prefs.sound) v.playSoundEffect(SoundEffectConstants.CLICK)
    }

    private fun perform(key: KbKey) {
        val l = listener ?: return
        when (val a = key.action) {
            is KbAction.Insert -> l.onInsert(a.text)
            KbAction.Shift -> l.onShiftTap()
            KbAction.Backspace -> l.onBackspace()
            KbAction.Enter -> l.onEnter()
            KbAction.Space -> l.onSpace()
            is KbAction.ShowLayer -> l.onShowLayer(a.layer)
            KbAction.Language -> l.onLanguage()
            KbAction.OpenHub -> l.onOpenHub()
            KbAction.OneHandToggle -> l.onOneHandToggle()
            KbAction.CursorLeft -> l.onCursorLeft()
            KbAction.CursorRight -> l.onCursorRight()
            KbAction.ToggleEnglish -> l.onToggleEnglish()
            KbAction.HideKeyboard -> l.onHideKeyboard()
            KbAction.OpenSettings -> l.onOpenSettings()
        }
    }

    // ---------------- 长按与预览 ----------------

    private fun scheduleLongPress(v: View) {
        val r = Runnable {
            val key = (v as? KeyButton)?.key ?: return@Runnable
            if (key.action is KbAction.Shift) {
                listener?.onShiftLongPress()
                return@Runnable
            }
            if (key.action is KbAction.Backspace) {
                dismissPreview()
                startRepeat(key)
                return@Runnable
            }
            val alts = key.longPress.ifEmpty { KbLayouts.alternatesFor(key.label) }
            if (alts.isNotEmpty()) {
                dismissPreview()
                showAlternates(v, alts)
            }
        }
        longPress = r
        handler.postDelayed(r, LONG_PRESS_MS)
    }

    /** 长按 ⌫：每 60ms 重复删一个字符（商用输入法标配）。 */
    private fun startRepeat(key: KbKey) {
        repeatRunnable = object : Runnable {
            override fun run() {
                if (key.action is KbAction.Backspace) listener?.onBackspace()
                handler.postDelayed(this, 60L)
            }
        }.also { handler.postDelayed(it, 60L) }
    }

    private fun stopRepeat() {
        repeatRunnable?.let { handler.removeCallbacks(it) }
        repeatRunnable = null
    }

    private fun cancelPendingLongPress() {
        longPress?.let { handler.removeCallbacks(it) }
        longPress = null
    }

    private fun showPreview(anchor: View) {
        if (!prefs.keyPreview) return
        dismissPreview() // 防止上一次气泡未销毁而叠加/残留
        val text = (anchor as? KeyButton)?.key?.label ?: return
        val popup = PopupWindow(
            TextView(context).apply {
                this.text = text
                gravity = Gravity.CENTER
                setTextColor(colors.keyText)
                background = plate(colors.key, dp(11), colors.glassBorder)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
            },
            dp(40),
            dp(52),
            false,
        ).apply {
            isOutsideTouchable = false
            setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
        }
        positionAbove(popup, anchor, dp(40), dp(52))
        preview = popup
        // 兜底：即便收不到 ACTION_UP（滑出/多指/窗口切换），600ms 后也必须消失
        handler.postDelayed(previewTimeout, PREVIEW_TIMEOUT_MS)
    }

    private val previewTimeout = Runnable { dismissPreview() }

    /** 供输入法服务在窗口隐藏/切换时强制清理所有悬浮窗。 */
    fun dismissAllPopups() = dismissPopups()

    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (visibility != View.VISIBLE) dismissPopups()
    }

    private fun showAlternates(anchor: View, items: List<String>) {
        val row = LinearLayout(context).apply {
            orientation = HORIZONTAL
            setBackgroundColor(colors.bg)
            setPadding(dp(4), dp(4), dp(4), dp(4))
        }
        for (item in items) {
            row.addView(
                TextView(context).apply {
                    text = item
                    gravity = Gravity.CENTER
                    setTextColor(colors.keyText)
                    background = plate(colors.key, dp(10), colors.glassBorder)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 20f)
                    setOnClickListener {
                        listener?.onInsert(item)
                        dismissAlternates()
                    }
                },
                LayoutParams(dp(44), dp(48)).apply { marginStart = dp(4) }
            )
        }
        val width = dp(52) * items.size + dp(8)
        val popup = PopupWindow(row, width, dp(56), false).apply {
            isOutsideTouchable = true
            setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
        }
        positionAbove(popup, anchor, width, dp(56))
        alternates = popup
    }

    private fun positionAbove(popup: PopupWindow, anchor: View, w: Int, h: Int) {
        val loc = IntArray(2)
        anchor.getLocationInWindow(loc)
        val x = loc[0] + anchor.width / 2 - w / 2
        val y = loc[1] - h - dp(4)
        try {
            popup.showAtLocation(this, Gravity.NO_GRAVITY, x.coerceAtLeast(0), y.coerceAtLeast(0))
        } catch (_: Exception) {
            // 窗口 token 失效（切换输入框瞬间）时静默忽略
        }
    }

    private fun dismissPreview() {
        handler.removeCallbacks(previewTimeout)
        preview?.dismiss()
        preview = null
    }

    private fun dismissAlternates() {
        alternates?.dismiss()
        alternates = null
    }

    private fun dismissPopups() {
        dismissPreview()
        dismissAlternates()
        cancelPendingLongPress()
        stopRepeat()
    }

    private fun dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()

    private companion object {
        const val LONG_PRESS_MS = 420L
        const val PREVIEW_TIMEOUT_MS = 600L
        const val SWIPE_THRESHOLD = 60f
        const val SWIPE_THRESHOLD_ORTHO = 90f

        /** 字母键 id 前缀（`letter_a`），大小写原地改写靠它取回小写字母 */
        const val KEY_PREFIX_LETTER = "letter_"
        /** Shift 键 id（字面 ⇧ / ⇪ 也要跟着状态换） */
        const val KEY_ID_SHIFT = "shift"
    }
}
