# 第三方数据与许可（THIRD PARTY NOTICES）

本仓库包含第三方数据，其许可与本项目代码（MIT）不同：

## CC-CEDICT（中英词典数据）

- 文件：`app/src/main/assets/en_dict.tsv`（由 `tools/gen_english_dict.py` 从 CC-CEDICT 生成）
- 上游：<https://www.mdbg.net/chinese/dictionary?page=cc-cedict>
- 许可：**Creative Commons Attribution-ShareAlike 4.0 International（CC BY-SA 4.0）**
  <https://creativecommons.org/licenses/by-sa/4.0/>
- 署名：CC-CEDICT 由社区维护并由 MDBG 发布（CC-CEDICT project, published by MDBG）。
- 说明：
  - 生成方式：`python3 tools/gen_english_dict.py --cedict <cedict_ts.u8> --out app/src/main/assets/en_dict.tsv`
    （下载 <https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.zip> 解压得到 `cedict_ts.u8`）
  - 我们对原始数据做了以下**修改**：仅保留 1–4 字纯中文词条；挑选单条最常用释义；去除
    `CL:`、`variant of`、`see …` 等词典标记；输出为 `简体<TAB>英文` 的 TSV。
  - 因为 CC BY-SA 4.0 具有"相同方式共享"要求：**该 TSV 数据文件及其衍生版本需继续以 CC BY-SA 4.0 分发**
    （本仓库代码仍为 MIT）。再分发本应用/本数据时请保留本文件与上述署名。
- 其它引用到的同类数据（未打包）：`inputx-pinyin` 的词库为其自身许可（MIT OR Apache-2.0，见该 crate 说明）。

## Tatoeba（双语例句数据，生成 sentbank.bin）

- 文件：`app/src/main/assets/sentbank.bin`（由 `tools/gen_sentbank.py` 从 Tatoeba 中英句对生成；CI 可选产物，缺失时句库候选为空、功能回退）
- 上游：<https://tatoeba.org/>
- 数据集：OPUS-Tatoeba `en-zh_cn`（v2023-04-12）<https://opus.nlpl.eu/production/corpus.php?corpus=Tatoeba>
- 许可：**Creative Commons Attribution 2.0 France（CC BY 2.0 FR）** <https://creativecommons.org/licenses/by/2.0/fr/>
- 署名：Sentences from **Tatoeba**（tatoeba.org）。再分发本应用或衍生数据时请保留本文件与该署名。
- 说明：仅保留对齐的中英句对并转为二进制句库，未对其它内容做修改。

## jieba 词表（生成 lex.bin）

- 文件：`app/src/main/assets/lex.bin`（由 `rust-core/src/bin/lexgen.rs` 从 jieba 词表生成；CI 可选产物，缺失时退回内置 16.5 万词库）
- 上游：<https://github.com/fxsjy/jieba>（`jieba/dict.txt`）
- 许可：随上游仓库以 **MIT** 分发 <https://github.com/fxsjy/jieba/blob/master/LICENSE>
- 署名：jieba（"结巴"中文分词）词表，作者 fxsjy 及贡献者。

## CEFR 词级分级表（生成 cefr.tsv）

- 文件：`app/src/main/assets/cefr.tsv`（一行一条 `词<TAB>级别`，共 8690 条 A1..C2；可选产物，缺失时分级统计为空、逐词译词不标级别）
- 上游（两个数据集合成）：
  1. **CEFR-J Vocabulary Profile v1.5** — <https://github.com/openlanguageprofiles/olp-en-cefrj>
     （`cefrj-vocabulary-profile-1.5.csv`）
  2. **Octanove Vocabulary Profile C1/C2 v1.0** — 同仓库 `octanove-vocabulary-profile-c1c2-1.0.csv`（补 C1/C2 两级）
- 许可：
  - CEFR-J：允许**用于研究与商业用途、不收费**，条件是**按要求署名**（见下方引用）。
    版权归 Tokyo University of Foreign Studies 的 Tono Laboratory（TUFU）。
    <https://github.com/openlanguageprofiles/olp-en-cefrj/blob/master/README.md>
  - Octanove C1/C2：**Creative Commons Attribution 4.0 International（CC BY 4.0）**
    <https://creativecommons.org/licenses/by/4.0/>
- 必需引用（上游 README 给定的格式）：
  > The CEFR-J Wordlist Version 1.5. Compiled by Yukio Tono, Tokyo University of Foreign
  > Studies. Retrieved from <http://www.cefr-j.org/download.html>
- 说明：合并两份表后**按词去重、同一词取最低（最易）级别**；斜杠分隔的变体
  （`a.m./A.M./am/AM`）拆开逐个收录；含空格的词组不收（逐词译词只按单词查）。
  CJK 标题、词性列与两份数据的其余字段未进入本文件。
