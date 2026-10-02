package com.typesake.app

import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import com.typesake.app.ui.GlassCard
import com.typesake.app.ui.TypesakeTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.LocalDate

/** 首页：状态 + 启用指引 + 试打 + 设置 + 隐私说明。 */
class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs = TypesakePrefs(this)
        setContent {
            var paletteId by remember { mutableIntStateOf(prefs.palette) }
            var ready by remember { mutableIntStateOf(0) }
            androidx.compose.runtime.LaunchedEffect(Unit) {
                withContext(Dispatchers.IO) {
                    TypesakeAssets.ensureEnglishDict(this@MainActivity)
                    TypesakeCore.init(filesDir.absolutePath)
                    TypesakeCore.setOptions(prefs.fuzzy, prefs.correction, prefs.shuangpin, prefs.script)
                }
                ready++
            }
            TypesakeTheme(paletteId, prefs.dynamicColor) {
                Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    SetupScreen(paletteId, ready) { paletteId = it }
                }
            }
        }
    }
}

@Composable
private fun SetupScreen(paletteId: Int, readyTick: Int, onPaletteChange: (Int) -> Unit) {
    val context = LocalContext.current
    val prefs = remember { TypesakePrefs(context) }

    var themeMode by remember { mutableIntStateOf(prefs.themeMode) }
    var keyHeight by remember { mutableFloatStateOf(prefs.keyHeightDp.toFloat()) }
    var haptics by remember { mutableStateOf(prefs.haptics) }
    var sound by remember { mutableStateOf(prefs.sound) }
    var preview by remember { mutableStateOf(prefs.keyPreview) }
    var clipboard by remember { mutableStateOf(prefs.clipboardHistory) }
    var palette by remember { mutableIntStateOf(paletteId) }
    var fuzzy by remember { mutableStateOf(prefs.fuzzy) }
    var correction by remember { mutableStateOf(prefs.correction) }
    var statTick by remember { mutableIntStateOf(readyTick) }
    var tick by remember { mutableIntStateOf(readyTick) }
    var shuangpin by remember { mutableIntStateOf(prefs.shuangpin) }
    var glossOn by remember { mutableStateOf(prefs.gloss) }
    var freshOn by remember { mutableStateOf(prefs.freshMark) }
    var shortcutOn by remember { mutableStateOf(prefs.shortcut) }
    var enFixOn by remember { mutableStateOf(prefs.englishFix) }
    var blockedApps by remember { mutableStateOf(prefs.blockedApps) }
    var vocabLevels by remember(tick) { mutableStateOf(TypesakeCore.vocabLevelStats()) }
    var words by remember(tick) { mutableStateOf(TypesakeCore.myWords()) }
    var names by remember(tick) { mutableStateOf(TypesakeCore.myNames()) }
    var script by remember { mutableIntStateOf(prefs.script) }
    var vertical by remember { mutableStateOf(prefs.verticalCandidates) }
    var pageKeys by remember { mutableIntStateOf(prefs.pageKeys) }
    var customSymbols by remember { mutableStateOf(prefs.customSymbols) }
    var hideNumberRow by remember { mutableStateOf(prefs.hideNumberRow) }
    var candIndex by remember { mutableStateOf(prefs.showCandidateIndex) }
    var hapticLevel by remember { mutableIntStateOf(prefs.hapticLevel) }
    var oneHandScale by remember { mutableIntStateOf(prefs.oneHandScale) }
    var dynamicColor by remember { mutableStateOf(prefs.dynamicColor) }
    var blocked by remember(tick) { mutableStateOf(TypesakeCore.blockedList()) }
    var t9 by remember { mutableStateOf(prefs.t9Layout) }
    var trial by remember { mutableStateOf("") }
    var savedFlash by remember { mutableStateOf("") }

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            Icon(
                painter = painterResource(R.drawable.ic_typesake_line),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(40.dp),
            )
            Text(
                "Typesake 输入法",
                style = MaterialTheme.typography.headlineMedium,
                modifier = Modifier.padding(start = 10.dp),
            )
        }
        Text(
            if (TypesakeCore.available) {
                "离线引擎已加载 · 词库 ${TypesakeCore.lexiconEntries()} 条"
            } else {
                "演示模式 · 安装包含 .so 后启用完整 Rust 引擎"
            },
            style = MaterialTheme.typography.bodySmall,
        )

        val stats = remember(statTick) { TypesakeCore.stats() }
        GlassCard {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("学习数据", style = MaterialTheme.typography.titleMedium)
                Text(
                    "已输入 ${stats.words} 词 · 连续 " +
                        "${TextUtils.streak(stats.days, LocalDate.now().toString())} 天 · " +
                        "收藏 ${stats.saved} · 拼音词库 ${stats.lex} · 扩展词库 ${stats.biglex} · 句库 ${stats.sentbank} · 英文词典 ${stats.endict} 条",
                    style = MaterialTheme.typography.bodyMedium,
                )
                val top = words.filter { it.picks > 0 }.sortedByDescending { it.picks }.take(5)
                if (top.isNotEmpty()) {
                    Text(
                        "最常用：" + top.joinToString("、") { "${it.word}(${it.picks})" },
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                Heatmap(stats.days)
                Text("最近 12 周输入活跃度", style = MaterialTheme.typography.bodySmall)
                Text("点此刷新", style = MaterialTheme.typography.bodySmall, modifier = Modifier.clickable { statTick++ })
            }
        }
        GlassCard {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("启用三步走", style = MaterialTheme.typography.titleMedium)
                Text("1. 打开系统输入法设置，启用「Typesake 输入法」")
                Text("2. 切换到 Typesake（键盘左下角 🌐 可随时切换）")
                Text("3. 任意输入框打拼音：空格上屏首选，双击空格收藏整句")
            }
        }
        OutlinedButton(
            onClick = { context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS)) },
            modifier = Modifier.fillMaxWidth(),
        ) { Text("去开启输入法") }
        OutlinedButton(
            onClick = { context.startActivity(Intent(context, HubActivity::class.java)) },
            modifier = Modifier.fillMaxWidth(),
        ) { Text("打开学习中心") }

        Spacer(Modifier.height(4.dp))
        Text("试打 & 收藏", style = MaterialTheme.typography.titleMedium)
        OutlinedTextField(
            value = trial,
            onValueChange = { trial = it },
            label = { Text("输入中文，如：谢谢 / 今天开会") },
            modifier = Modifier.fillMaxWidth(),
        )
        val english = remember(trial) { TypesakeCore.suggest(trial) }
        GlassCard {
            Column(Modifier.padding(14.dp)) {
                Text("英文伴学", style = MaterialTheme.typography.titleSmall)
                Text(
                    if (english.isBlank()) "（暂无对应表达，换个常用说法试试）" else english,
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }
        Button(
            onClick = {
                val cn = trial.trim()
                if (cn.isNotEmpty()) {
                    val ok = TypesakeCore.save(cn, TypesakeCore.suggest(cn))
                    savedFlash = if (ok) "已收藏：$cn" else "收藏失败"
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text("★ 收藏这句") }
        if (savedFlash.isNotEmpty()) {
            Text(savedFlash, style = MaterialTheme.typography.bodySmall)
        }

        Spacer(Modifier.height(4.dp))
        if (blocked.isNotEmpty()) {
            Text("已删词（${blocked.size}）", style = MaterialTheme.typography.titleMedium)
            Text("删掉的候选不会再出现；这里可以恢复。", style = MaterialTheme.typography.bodySmall)
            GlassCard {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    blocked.take(8).forEach { (py, w) ->
                        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Text("$py → $w", style = MaterialTheme.typography.bodyMedium)
                            Text("恢复", style = MaterialTheme.typography.bodySmall, modifier = Modifier.clickable {
                                TypesakeCore.restoreWord(py, w)
                                blocked = TypesakeCore.blockedList()
                            })
                        }
                    }
                    if (blocked.size > 8) {
                        Text("…还有 ${blocked.size - 8} 条", style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }

        Text("名字词库（${names.size}）", style = MaterialTheme.typography.titleMedium)
        Text(
            "从通讯录/微信复制一串姓名（换行、逗号或空格分隔），粘贴进来即可用拼音联想。" +
                "本应用未申请通讯录权限，导入完全由你手动触发。",
            style = MaterialTheme.typography.bodySmall,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = {
                val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                val text = cm?.primaryClip?.takeIf { it.itemCount > 0 }
                    ?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty()
                val added = TypesakeCore.addNames(text)
                names = TypesakeCore.myNames()
                savedFlash = if (added > 0) "已导入 $added 个姓名" else "没解析到姓名（或已存在）"
            }) { Text("从剪贴板导入名单") }
            OutlinedButton(onClick = {
                TypesakeCore.clearNames()
                names = TypesakeCore.myNames()
                savedFlash = "名字词库已清空"
            }) { Text("清空名字") }
        }

        Text("我的词库（${words.size}）", style = MaterialTheme.typography.titleMedium)
        Text(
            "连续选同一个词 3 次会自动置顶；这里可以删除或清空学习记录。",
            style = MaterialTheme.typography.bodySmall,
        )
        GlassCard {
            Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (words.isEmpty()) {
                    Text("还没有学习记录：多打几个词，或长按候选「置顶」。", style = MaterialTheme.typography.bodySmall)
                }
                words.take(12).forEach { w ->
                    Row(
                        Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(
                            "${w.pinyin} → ${w.word}" + if (w.picks == 0) "（置顶）" else "（${w.picks} 次）",
                            style = MaterialTheme.typography.bodyMedium,
                        )
                        Text(
                            "删除",
                            style = MaterialTheme.typography.bodySmall,
                            modifier = Modifier.clickable {
                                TypesakeCore.forget(w.pinyin, w.word)
                                words = TypesakeCore.myWords()
                            },
                        )
                    }
                }
                if (words.size > 12) {
                    Text("…还有 ${words.size - 12} 条", style = MaterialTheme.typography.bodySmall)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = {
                        TypesakeCore.resetLearning()
                        words = TypesakeCore.myWords()
                        savedFlash = "已清空学习记录"
                    }) { Text("清空学习记录") }
                    OutlinedButton(onClick = {
                        val json = TypesakeCore.backupJson()
                        if (json.isNotEmpty()) {
                            val send = Intent(Intent.ACTION_SEND).apply {
                                type = "text/plain"
                                putExtra(Intent.EXTRA_TEXT, json)
                            }
                            context.startActivity(Intent.createChooser(send, "备份词库"))
                        }
                    }) { Text("分享备份") }
                    OutlinedButton(onClick = {
                        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                        val text = cm?.primaryClip?.takeIf { it.itemCount > 0 }
                            ?.getItemAt(0)?.coerceToText(context)?.toString().orEmpty()
                        val ok = TypesakeCore.restoreJson(text)
                        words = TypesakeCore.myWords()
                        savedFlash = if (ok) "已从剪贴板恢复词库" else "剪贴板里没有可用的备份 JSON"
                    }) { Text("剪贴板恢复") }
                }
            }
        }

        Text("键盘设置", style = MaterialTheme.typography.titleMedium)
        Text("主题", style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("跟随系统" to 0, "浅色" to 1, "深色" to 2).forEach { (label, mode) ->
                val selected = themeMode == mode
                if (selected) {
                    Button(onClick = {}) { Text(label) }
                } else {
                    OutlinedButton(onClick = {
                        themeMode = mode
                        prefs.themeMode = mode
                    }) { Text(label) }
                }
            }
        }
        Text("配色", style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("珊瑚" to 0, "翠绿" to 1, "暖阳" to 2, "玫瑰" to 3, "海洋" to 4).forEach { (label, id) ->
                if (palette == id) {
                    Button(onClick = {}) { Text(label) }
                } else {
                    OutlinedButton(onClick = {
                        palette = id
                        prefs.palette = id
                        onPaletteChange(id)
                    }) { Text(label) }
                }
            }
        }
        // 输入方案：清单来自引擎（`shuangpin::schemes()`），不在 UI 里写死
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            TypesakeCore.schemes().chunked(3).forEach { rowItems ->
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    rowItems.forEach { s ->
                        if (shuangpin == s.id) {
                            Button(onClick = {}) { Text(s.name) }
                        } else {
                            OutlinedButton(onClick = {
                                shuangpin = s.id
                                prefs.shuangpin = s.id
                                TypesakeCore.setOptions(fuzzy, correction, s.id, script)
                            }) { Text(s.name) }
                        }
                    }
                }
            }
        }
        SwitchRow("候选旁逐词译词", glossOn) {
            glossOn = it
            prefs.gloss = it
            TypesakeCore.setFeatures(glossOn, freshOn, shortcutOn, enFixOn)
        }
        SwitchRow("生词橙标（还没上屏过的词）", freshOn) {
            freshOn = it
            prefs.freshMark = it
            TypesakeCore.setFeatures(glossOn, freshOn, shortcutOn, enFixOn)
        }
        SwitchRow("快捷输入（v 算式 / i 中文数字 / u 码点）", shortcutOn) {
            shortcutOn = it
            prefs.shortcut = it
            TypesakeCore.setFeatures(glossOn, freshOn, shortcutOn, enFixOn)
        }
        SwitchRow("英文模式拼写纠正", enFixOn) {
            enFixOn = it
            prefs.englishFix = it
            TypesakeCore.setFeatures(glossOn, freshOn, shortcutOn, enFixOn)
        }
        Text("词汇分级统计（CEFR）", style = MaterialTheme.typography.bodyMedium)
        if (vocabLevels.isEmpty()) {
            Text(
                "词表未载入：把 cefr.tsv 放进词典目录后重启输入法。",
                style = MaterialTheme.typography.bodySmall,
            )
        } else {
            Text(
                vocabLevels.joinToString("  ") { (lv, n) -> "$lv $n" },
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Text("屏蔽英文候选的应用（包名，每行一个）", style = MaterialTheme.typography.bodyMedium)
        OutlinedTextField(
            value = blockedApps,
            onValueChange = { v ->
                blockedApps = v
                prefs.blockedApps = v
            },
            modifier = Modifier.fillMaxWidth(),
            minLines = 2,
            maxLines = 4,
            placeholder = { Text("com.example.notes") },
            supportingText = { Text("留空 = 所有应用都给英文候选") },
        )
        var autoSpace by remember { mutableStateOf(prefs.englishAutoSpace) }
        SwitchRow("英文上屏后自动空格", autoSpace) {
            autoSpace = it
            prefs.englishAutoSpace = it
        }
        OutlinedButton(
            onClick = {
                val a = TypesakeCore.analyze("nihao")
                val b = TypesakeCore.analyze("zhongguo")
                val c = TypesakeCore.analyze("jintiankaihui")
                savedFlash = "自检：引擎=${if (TypesakeCore.available) "已加载" else "未加载"}" +
                    (TypesakeCore.stats().lastError.takeIf { it.isNotEmpty() }?.let { "｜错误：$it" } ?: "") + "｜" +
                    "nihao→${a.candidates.take(2)}｜zhongguo→${b.candidates.take(2)}｜" +
                    "jintian/kaihui→${c.candidates.take(2)}"
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text("引擎自检（候选排查）") }
        SwitchRow("输出繁体（简繁转换）", script == 1) {
            script = if (it) 1 else 0
            prefs.script = script
            TypesakeCore.setOptions(fuzzy, correction, shuangpin, script)
        }
        SwitchRow("隐藏数字行（上滑出数字/符号）", hideNumberRow) {
            hideNumberRow = it
            prefs.hideNumberRow = it
        }
        SwitchRow("候选显示 1/2/3 序号", candIndex) {
            candIndex = it
            prefs.showCandidateIndex = it
        }
        SwitchRow("跟随壁纸取色（Android 12+）", dynamicColor) {
            dynamicColor = it
            prefs.dynamicColor = it
        }
        Text("震动强度", style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("关" to 0, "弱" to 1, "中" to 2, "强" to 3).forEach { (label, id) ->
                if (hapticLevel == id) {
                    Button(onClick = {}) { Text(label) }
                } else {
                    OutlinedButton(onClick = { hapticLevel = id; prefs.hapticLevel = id }) { Text(label) }
                }
            }
        }
        Text("单手键盘宽度 ${oneHandScale}%", style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(75, 80, 85).forEach { pct ->
                if (oneHandScale == pct) {
                    Button(onClick = {}) { Text("$pct%") }
                } else {
                    OutlinedButton(onClick = { oneHandScale = pct; prefs.oneHandScale = pct }) { Text("$pct%") }
                }
            }
        }
        SwitchRow("候选竖排", vertical) {
            vertical = it
            prefs.verticalCandidates = it
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("逗号句号=二三候选" to 0, "逗号句号=翻页" to 1).forEach { (label, id) ->
                if (pageKeys == id) {
                    Button(onClick = {}) { Text(label) }
                } else {
                    OutlinedButton(onClick = {
                        pageKeys = id
                        prefs.pageKeys = id
                    }) { Text(label) }
                }
            }
        }
        OutlinedTextField(
            value = customSymbols,
            onValueChange = { customSymbols = it },
            label = { Text("自定义符号（符号页第一行，最多 10 个）") },
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedButton(onClick = { prefs.customSymbols = customSymbols; savedFlash = "符号已保存" }) {
            Text("保存符号")
        }
        SwitchRow("九键输入（T9）", t9) {
            t9 = it
            prefs.t9Layout = it
        }
        SwitchRow("模糊音（z/zh、n/l、an/ang…）", fuzzy) {
            fuzzy = it
            prefs.fuzzy = it
            TypesakeCore.setOptions(it, correction, shuangpin, script)
        }
        SwitchRow("击键纠错（邻键/漏键/多键/换位）", correction) {
            correction = it
            prefs.correction = it
            TypesakeCore.setOptions(fuzzy, it, shuangpin, script)
        }
        Text("按键高度 ${keyHeight.toInt()} dp", style = MaterialTheme.typography.bodyMedium)
        Slider(
            value = keyHeight,
            onValueChange = { keyHeight = it },
            onValueChangeFinished = { prefs.keyHeightDp = keyHeight.toInt() },
            valueRange = TypesakePrefs.MIN_KEY_HEIGHT.toFloat()..TypesakePrefs.MAX_KEY_HEIGHT.toFloat(),
            steps = TypesakePrefs.MAX_KEY_HEIGHT - TypesakePrefs.MIN_KEY_HEIGHT - 1,
            modifier = Modifier.fillMaxWidth(),
        )
        SwitchRow("按键震动", haptics) { haptics = it; prefs.haptics = it }
        SwitchRow("按键声音", sound) { sound = it; prefs.sound = it }
        SwitchRow("按键预览气泡", preview) { preview = it; prefs.keyPreview = it }
        SwitchRow("剪贴板历史（仅本机内存）", clipboard) {
            clipboard = it
            prefs.clipboardHistory = it
        }
        Text(
            "单手模式在键盘 ?123 页的「⇤」键切换，自动记住选择。",
            style = MaterialTheme.typography.bodySmall,
        )

        Spacer(Modifier.height(4.dp))
        GlassCard {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text("隐私说明", style = MaterialTheme.typography.titleSmall)
                Text("· 本应用不申请网络权限，输入内容不出设备")
                Text("· 拼音转换、英文表达、词频学习全部本地完成")
                Text("· 密码等隐私输入框自动关闭转换、学习与收藏")
                Text("· 剪贴板历史仅保存在内存，退出即清空")
                Text("· 英文词典数据来自 CC-CEDICT（CC BY-SA 4.0，署名见仓库 THIRD_PARTY_NOTICES.md）")
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

/** B5 输入热力图（最近 12 周，每格一天）。 */
@Composable
private fun Heatmap(days: List<String>, weeks: Int = 12) {
    val active = remember(days) { days.toHashSet() }
    val today = LocalDate.now()
    val start = today.minusDays(((weeks - 1) * 7 + (today.dayOfWeek.value % 7)).toLong())
    val onColor = MaterialTheme.colorScheme.primary
    val offColor = MaterialTheme.colorScheme.surfaceVariant
    Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        for (w in 0 until weeks) {
            Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                for (d in 0 until 7) {
                    val date = start.plusDays((w * 7 + d).toLong())
                    val on = !date.isAfter(today) && active.contains(date.toString())
                    Box(
                        Modifier
                            .size(11.dp)
                            .clip(RoundedCornerShape(3.dp))
                            .background(if (on) onColor else offColor)
                    )
                }
            }
        }
    }
}

@Composable
private fun SwitchRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, Modifier.padding(top = 12.dp), style = MaterialTheme.typography.bodyMedium)
        Switch(checked = checked, onCheckedChange = onChange)
    }
}
