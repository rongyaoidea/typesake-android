package com.typesake.app

import android.content.Context
import java.io.File

/**
 * 把打包在 assets 里的离线词典（CC-CEDICT 生成的 en_dict.tsv 等）拷到 filesDir，
 * Rust 引擎启动时从该路径载入。版本升级时自动重拷。
 *
 * 放在 assets 而不是编进 .so：APK 里只存一份（且被压缩），三个 ABI 不再各背一份。
 *
 * 文件分两类：
 * - [CORE]：随包必有，版本号门禁只看它们（三者齐全 + 版本一致 = 不重拷）；
 * - [OPTIONAL]：CI 可选产物（本地构建/词库 job 失败时不存在），缺了不算失败、
 *   出现时按需补拷——不能让它们把版本号打回去导致每次启动重拷 3.6MB。
 */
object TypesakeAssets {

    /** 核心文件：(assets 名, filesDir 名)，一定随 APK 存在 */
    private val CORE = listOf(
        "en_dict.tsv" to "en_dict.tsv",
        "s2t.tsv" to "s2t.tsv",
        "t2s.tsv" to "t2s.tsv",
    )

    /** CI 可选产物：缺失不算失败，出现时拷入 */
    private val OPTIONAL = listOf(
        "lex.bin" to "lex.bin",
        "gram.bin" to "gram.bin",
        "sentbank.bin" to "sentbank.bin",
        "cefr.tsv" to "cefr.tsv",
    )

    private const val KEY_DICT_VERSION = "en_dict_version"

    private fun ready(dir: File, name: String): Boolean =
        File(dir, name).let { f -> f.exists() && f.length() > 0 }

    /** 确保随包数据（英文词典 + 简繁表 + 可选词库）就绪；返回核心文件是否全部可读。 */
    fun ensureEnglishDict(context: Context): Boolean {
        val prefs = context.getSharedPreferences("typesake_prefs", Context.MODE_PRIVATE)
        val version = BuildConfig.VERSION_CODE
        val dir = context.filesDir
        val sameVersion = prefs.getInt(KEY_DICT_VERSION, -1) == version
        val coreReady = CORE.all { ready(dir, it.second) }

        // 核心文件：版本未变且已就绪则整组跳过（避免每次启动重拷）
        if (!(sameVersion && coreReady)) {
            for ((asset, name) in CORE) {
                val target = File(dir, name)
                try {
                    context.assets.open(asset).use { input ->
                        target.outputStream().use { output -> input.copyTo(output) }
                    }
                } catch (_: Exception) {
                    // 不删旧文件：重拷失败时保住上一份可用数据，下次启动再试
                    // （版本号未写入，失败会重试）
                }
            }
        }

        // 可选文件：版本变了就整组重拷；版本没变则只补拷缺失的那个
        for ((asset, name) in OPTIONAL) {
            if (sameVersion && ready(dir, name)) continue
            try {
                context.assets.open(asset).use { input ->
                    val target = File(dir, name)
                    target.outputStream().use { output -> input.copyTo(output) }
                }
            } catch (_: Exception) {
                // asset 不存在属正常情况（本地构建或 CI 词库 job 失败），保持原样
            }
        }

        val ok = CORE.all { ready(dir, it.second) }
        if (ok && !sameVersion) prefs.edit().putInt(KEY_DICT_VERSION, version).apply()
        return ok
    }
}
