package com.typesake.app

import android.os.Bundle
import android.speech.tts.TextToSpeech
import java.util.Locale
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.TextButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import com.typesake.app.ui.GlassCard
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import com.typesake.app.ui.TypesakeTheme

/** 学习中心：收藏列表 + 点击看语法讲解 + 清空（回到前台自动刷新）。 */
class HubActivity : ComponentActivity() {

    private var tick by mutableIntStateOf(0)
    private var tts: TextToSpeech? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        lifecycleScope.launch {
            withContext(Dispatchers.IO) {
                TypesakeAssets.ensureEnglishDict(this@HubActivity)
                TypesakeCore.init(filesDir.absolutePath)
            }
            tick++
        }
        tts = TextToSpeech(this) { status ->
            // TTS 引擎异步就绪：在回调里设语言 + 检查离线可用性。
            // 系统引擎可能联网下载语音/云合成，README 已改成“运行时零网络请求（不含系统 TTS）”。
            if (status == TextToSpeech.SUCCESS) {
                val res = tts?.setLanguage(Locale.US)
                if (res == TextToSpeech.LANG_MISSING_DATA || res == TextToSpeech.LANG_NOT_SUPPORTED) {
                    tts?.setLanguage(Locale.getDefault())
                }
            }
        }
        setContent {
            TypesakeTheme(TypesakePrefs(this@HubActivity).palette) {
                Surface(Modifier.fillMaxSize()) {
                    var saved by remember(tick) { mutableStateOf(TypesakeCore.list()) }
                    var expanded by remember { mutableStateOf<TypesakeCore.SavedPhrase?>(null) }
                    var editing by remember { mutableStateOf<TypesakeCore.SavedPhrase?>(null) }
                    var editEn by remember { mutableStateOf("") }
                    Column(
                        Modifier.fillMaxSize().padding(20.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        Row(
                            Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text(
                                "学习中心（${saved.size}）",
                                style = MaterialTheme.typography.headlineSmall,
                            )
                            OutlinedButton(onClick = {
                                TypesakeCore.clear()
                                saved = TypesakeCore.list()
                            }) { Text("清空") }
                        }
                        val stats = remember(tick) { TypesakeCore.stats() }
                        Text(
                            "已输入 ${stats.words} 词 · 连续 " +
                                "${TextUtils.streak(stats.days, java.time.LocalDate.now().toString())} 天 · " +
                                "收藏 ${stats.saved} · 词库 ${stats.lex} 条",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        if (saved.isEmpty()) {
                            GlassCard(Modifier.fillMaxWidth()) {
                                Text(
                                    "还没有收藏。去键盘打拼音，点 ★ 或双击空格收藏；\n" +
                                        "例如：nihao → 你好 → Hello!",
                                    Modifier.padding(16.dp),
                                )
                            }
                        }
                        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            items(saved, key = { "${it.chinese}\n${it.english}\n${it.saved_at}" }) { p ->
                                GlassCard(
                                    Modifier.fillMaxWidth().clickable {
                                        expanded = if (expanded == p) null else p
                                    }
                                ) {
                                    Column(
                                        Modifier.padding(14.dp),
                                        verticalArrangement = Arrangement.spacedBy(4.dp),
                                    ) {
                                        Text(p.chinese, style = MaterialTheme.typography.titleMedium)
                                        Text(
                                            if (p.english.isBlank()) "（英文待补）" else p.english,
                                            style = MaterialTheme.typography.bodyLarge,
                                        )
                                        if (expanded == p) {
                                            Text(
                                                TypesakeCore.explain(p.english.ifBlank { p.chinese }),
                                                style = MaterialTheme.typography.bodySmall,
                                            )
                                        } else {
                                            Text(
                                                "点我看语法讲解",
                                                style = MaterialTheme.typography.bodySmall,
                                            )
                                        }
                                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                            TextButton(onClick = { speak(p.english.ifBlank { p.chinese }) }) {
                                                Text("🔊 朗读")
                                            }
                                            TextButton(onClick = {
                                                editing = p
                                                editEn = p.english
                                            }) { Text("编辑英文") }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        tick++
    }

    override fun onDestroy() {
        tts?.stop()
        tts?.shutdown()
        super.onDestroy()
    }

    private fun speak(text: String) {
        if (text.isBlank()) return
        tts?.speak(text, TextToSpeech.QUEUE_FLUSH, null, "typesake")
    }
}
