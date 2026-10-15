//! The fonts the menus' message files are drawn with (see docs/assets/text.md section 4): what
//! each glyph number draws in Japanese, Korean and the English font's added accented letters, and
//! the English font texture with those letters drawn into it.
//!
//! A glyph is a 16 by 20 cell of the language's font texture, 32 a row (GetRectFontTex, font.cpp).
//! English and Korean have a table on the disc giving each glyph's character (`meswin/fonttbl_1.bin`,
//! `meswin/fonttbl_6.bin`); Japanese has none: the PAL disc's `fonttbl.bin` holds only ASCII. Its
//! numbering was read off the four Japanese font pages (`meswin/FontTex_0.tm2` to `FontTex_3.tm2`)
//! glyph by glyph: ASCII, then the full-width characters in Shift-JIS order (symbols, digits and
//! letters, kana, the JIS level 1 kanji the game uses) up to glyph 1921, then 22 characters added
//! later in Unicode order. Every Japanese message file of the game reads as Japanese with it (the
//! corpus test checks every glyph they use is in it and the pages have ink exactly where it has
//! characters). It is a list of characters, not the font: the pictures stay on the user's disc.

use std::collections::BTreeSet;

use crate::{bail, Result};

/// The characters of Japanese glyphs 94 on (glyph 94 is the full-width space).
pub const JAPANESE: &str = concat!(
    "　、。，．・：？！々ー―／～…‥’“”（）〔〕「」『』【】＋－±", // 94
    "×＞°％＆＠★○●□■△※→←↑↓♪０１２３４５６７８９ＡＢＣＤ",        // 126
    "ＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺａｂｃｄｅｆｇｈｉｊ", // 158
    "ｋｌｍｎｏｐｑｒｓｔｕｖｗｘｙｚぁあぃいぅうぇえぉおかがきぎくぐ", // 190
    "けげこごさざしじすずせぜそぞただちぢっつづてでとどなにぬねのはば", // 222
    "ぱひびぴふぶぷへべぺほぼぽまみむめもゃやゅゆょよらりるれろゎわゐ", // 254
    "ゑをんァアィイゥウェエォオカガキギクグケゲコゴサザシジスズセゼソ", // 286
    "ゾタダチヂッツヅテデトドナニヌネノハバパヒビピフブプヘベペホボポ", // 318
    "マミムメモャヤュユョヨラリルレロヮワヰヱヲンヴヵヶ─━ⅡⅢⅣ亜哀", // 350
    "愛挨悪握圧扱安暗案闇以位偉囲威尉意易為異移維胃衣違遺医井育一壱溢", // 382
    "稲印員因引飲院陰隠右羽雨渦嘘噂運雲影映栄永泳英衛鋭液益駅越円園怨", // 414
    "援演炎煙猿遠汚央奥応押横殴王黄屋憶臆俺恩温穏音下化仮何価加可夏嫁", // 446
    "家科暇果歌火箇花荷華菓課貨過蚊我牙画芽雅介会解回塊壊快怪悔懐戒改", // 478
    "晦械海界皆絵開階貝外害概街鎧骸蛙嚇各拡格獲確覚角較隔革学楽額掛割", // 510
    "括活渇叶株噛乾寒勘巻完官寛干幹患感慣換敢棺汗環甘看竿管簡缶肝艦観", // 542
    "貫還間関館丸含岸眼岩頑顔願企伎危喜器基奇嬉寄岐希忌揮机旗既期機帰", // 574
    "気祈季規記貴起輝騎鬼亀儀技犠疑義議吉詰却客逆丘久仇休及吸宮急救朽", // 606
    "求泣球究級給旧牛去居巨拒拠挙虚許距魚供競共凶協叫境強怯恐挟教橋況", // 638
    "狂狭胸脅興鏡響驚仰凝業局曲極玉勤均禁筋緊謹近金銀九句区苦駆具愚喰", // 670
    "空隅屈掘窟靴繰君訓群軍係傾刑兄型形径恵憩掲携敬景渓稽系経継計警軽", // 702
    "鶏芸迎劇撃激隙桁傑欠決穴結血訣月件健兼券剣堅嫌建懸拳検権犬研肩見", // 734
    "賢軒遣険験元原厳幻弦減源現言限個古呼固孤己庫戸故枯湖狐股虎誇雇五", // 766
    "互後御悟語誤護交光公功効厚口向喉好工幸広康抗攻更校構江港甲皇硬紅", // 798
    "耕考航荒行鉱鋼降項香高号合豪轟刻告国酷黒獄腰惚骨込頃今困婚恨昏昆", // 830
    "根混魂佐叉左差査砂鎖座催再最塞妻才採栽歳済砕祭細菜裁載際在材罪財", // 862
    "咲作削昨柵策索察拶撮札殺雑皿三傘参山惨散桟産讃斬残仕使刺司史四士", // 894
    "始姉姿子市師志思指支施枝止死氏獅私糸紙紫脂視詞詩試誌資飼歯事似字", // 926
    "寺慈持時次治示耳自辞鹿式識軸七執失嫉室湿疾質実写射捨赦煮社者謝車", // 958
    "蛇邪借尺灼爵若寂弱主取守手殊狩珠種趣酒首受呪寿授樹収周修拾秀秋終", // 990
    "習臭衆襲讐蹴週集醜住充十従柔獣縦重銃宿祝縮粛出術春瞬楯準潤盾純順", // 1022
    "処初所暑緒書助女序徐除傷償勝匠召商唱将小少床彰招掌昇晶松沼消渉焼", // 1054
    "焦照症省称章笑紹肖衝証詳象賞鐘障上丈乗冗剰城場嬢常情条杖浄状蒸譲", // 1086
    "飾拭植織職色触食尻伸信侵唇寝審心慎振新森浸深申真神紳親診身辛進針", // 1118
    "震人刃尋尽訊図吹帥推水睡粋衰遂随崇数枢澄世是凄制勢征性成整星晴正", // 1150
    "清牲生盛精聖声製西誠誓逝青静脆席惜昔析石積籍責赤跡切接摂折設節説", // 1182
    "雪絶舌先千占宣専川戦扇泉浅洗染潜線繊羨船選鮮前然全狙祖粗素組蘇訴", // 1214
    "阻創双倉奏爽層想捜掃操早曹巣槍槽燥争痩相窓総草葬蒼装走送遭騒像増", // 1246
    "憎臓蔵贈造側即息束測足速属賊族続揃存孫尊損村他多太打駄体対耐帯待", // 1278
    "怠態替泰袋貸退隊代台大第題鷹滝宅択濯託叩達奪脱辿棚谷樽誰丹単嘆担", // 1310
    "探炭短端誕鍛団壇弾断暖段男談値知地恥池置蜘遅築畜蓄茶着中仲宙忠昼", // 1342
    "柱注虫猪貯丁兆帳張彫徴挑朝潮町眺聴蝶調超跳長頂鳥直沈珍墜槌追痛通", // 1374
    "潰爪吊釣低停定帝底庭弟抵程締諦邸釘泥摘敵滴的適撤鉄天展店添貼転点", // 1406
    "伝殿電吐塗妬渡登途都砥努度土奴怒倒党冬凍刀塔島投搭東盗湯灯当等答", // 1438
    "糖統到討踏逃透頭騰闘働動同堂導憧洞瞳童胴道銅得徳特毒独読突届沌豚", // 1470
    "鈍内謎鍋縄南軟難汝二匂肉虹日乳入如任忍認猫熱年念燃粘之悩濃納能脳", // 1502
    "農把覇波派破婆馬廃拝敗杯背輩配倍培買売秤伯剥博白薄迫漠爆麦箱筈肌", // 1534
    "畑八発髪抜伴判半反板版犯繁般販範飯晩番盤蛮卑彼悲扉比疲皮碑秘肥被", // 1566
    "費避非飛備尾微眉美鼻匹必姫百標氷漂表評描病秒苗品浜貧敏不付夫婦冨", // 1598
    "布怖敷斧普浮父符腐負赴侮撫武舞部封風副復幅服福腹複覆淵払沸物分噴", // 1630
    "奮粉紛雰文聞兵平柄並閉米壁癖別変片編辺返便勉弁保捕歩補募墓慕暮母", // 1662
    "包呆報奉宝崩抱捧放方法砲胞褒訪豊飽乏亡坊妨帽忘忙房暴望棒冒肪膨謀", // 1694
    "貌防吠北僕墨朴牧没堀本翻盆磨魔埋妹枚毎幕膜枕抹末万慢満味未魅岬密", // 1726
    "妙民眠務夢無娘冥名命明迷銘鳴滅面模茂毛猛盲網木黙目戻貰問紋門夜野", // 1758
    "厄役約薬訳躍愉油癒唯優勇友幽憂有由裕誘遊雄融夕予余与誉幼妖容揺曜", // 1790
    "様洋溶用羊葉要遥陽養抑欲浴翌翼羅裸来頼雷絡落乱卵嵐覧利履理裏里離", // 1822
    "陸律率立略流留硫竜龍侶慮旅了両料涼療良量力緑淋臨輪隣鱗涙類令例冷", // 1854
    "励礼零霊麗暦歴列烈裂恋練蓮連呂炉路露労廊浪牢狼老郎六録論和話歪惑", // 1886
    "枠鷲湾腕儚剝哺嗅懺掟棍殲毯狡猾璧甦竈絆絨綺繚罠蛛袂騙",      // 1918
);

/// Japanese glyphs 0 to 1943: ASCII without the backslash, then JAPANESE.
pub const JAPANESE_GLYPHS: usize = 94 + 1850;

/// The English font texture the PAL game loads (LoadFontTexture, mainloop.cpp).
pub const ENGLISH_TEXTURE: &str = "meswin/FontTex_1_0.tm2";
/// The French, German, Italian and Spanish font texture, whose accented letters are glyphs 0x5E to
/// 0x9C (GetAlphabeticalFontNo_us, font.cpp).
pub const EUROPEAN_TEXTURE: &str = "meswin/FontTex_2_0.tm2";
/// The Korean font table.
pub const KOREAN_TABLE: &str = "meswin/fonttbl_6.bin";

/// The letters glyphs 288 to 319 (ENGLISH_EXTRA_FIRST on) of the English font read as, in this
/// order, the ones the French, German, Italian and Spanish text uses most: what an exported text
/// reads them as, and what the build draws there unless a mod's English text uses other letters
/// (english_slots). Their pictures are the European font's own, copied in at build time. The
/// English texture is loaded into a buffer of 0xD000 bytes (font_buff, mainloop.cpp), which leaves
/// room for one more row of 32 glyphs and no more.
pub const ENGLISH_EXTRA: &str = "¡¿ÀÁÄÇÈÉÑÖÜßàáâäçèéêëíîïñóôöùúûü";
pub const ENGLISH_EXTRA_FIRST: u16 = 288;
/// How many glyphs the English font can gain.
pub const ENGLISH_EXTRA_COUNT: usize = 32;
/// The buffer the game copies the English font texture into.
pub const FONT_BUFFER: usize = 0xD000;

/// The European fonts' letters at glyphs 0x5E to 0x9C, in glyph order, as the Latin-1 bytes the
/// menus write them with (FF00 + the byte; 0xBD and 0xBE are Œ and œ).
pub const EUROPEAN_LETTERS: [u8; 63] = [
    0xA1, 0xAA, 0xB0, 0xBA, 0xBF, 0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD,
    0xCE, 0xCF, 0xD1, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD9, 0xDA, 0xDB, 0xDC, 0xDD, 0xDF, 0xE0, 0xE1, 0xE2, 0xE3, 0xE4,
    0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xEB, 0xEC, 0xED, 0xEE, 0xEF, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF9, 0xFA,
    0xFB, 0xFC, 0xFD, 0xFF, 0xBD, 0xBE,
];

/// The character of a European letter's Latin-1 byte.
pub fn latin_char(byte: u8) -> char {
    match byte {
        0xBD => 'Œ',
        0xBE => 'œ',
        byte => byte as char,
    }
}

/// The European glyph that draws a character, of the letters at 0x5E on.
pub fn european_glyph(c: char) -> Option<u16> {
    EUROPEAN_LETTERS.iter().position(|&byte| latin_char(byte) == c).map(|at| 0x5E + at as u16)
}

/// A mark the build copies off a European letter to draw a letter no font has: over the base
/// letter, or under it (the cedilla).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Grave,
    Acute,
    Circumflex,
    Tilde,
    Diaeresis,
    Ring,
    Cedilla,
}

impl Mark {
    pub fn name(self) -> &'static str {
        match self {
            Mark::Grave => "grave",
            Mark::Acute => "acute",
            Mark::Circumflex => "circumflex",
            Mark::Tilde => "tilde",
            Mark::Diaeresis => "diaeresis",
            Mark::Ring => "ring",
            Mark::Cedilla => "cedilla",
        }
    }

    /// The European letters the mark is copied from, small and capital, each with its base letter.
    fn sources(self) -> [(char, char); 2] {
        match self {
            Mark::Grave => [('è', 'e'), ('È', 'E')],
            Mark::Acute => [('é', 'e'), ('É', 'E')],
            Mark::Circumflex => [('ê', 'e'), ('Ê', 'E')],
            Mark::Tilde => [('õ', 'o'), ('Õ', 'O')],
            Mark::Diaeresis => [('ë', 'e'), ('Ë', 'E')],
            Mark::Ring => [('å', 'a'), ('Å', 'A')],
            Mark::Cedilla => [('ç', 'c'), ('Ç', 'C')],
        }
    }

    fn below(self) -> bool {
        self == Mark::Cedilla
    }
}

/// The Latin-1 and Latin Extended-A letters that are an ASCII letter and one mark (their Unicode
/// canonical decomposition): (letters, their base letters, the mark when a European letter has it
/// to copy, the mark's name).
const DECOMPOSITIONS: [(&str, &str, Option<Mark>, &str); 13] = [
    ("ÀÈÌÒÙàèìòù", "AEIOUaeiou", Some(Mark::Grave), "grave"),
    ("ÁÉÍÓÚÝáéíóúýĆćĹĺŃńŔŕŚśŹź", "AEIOUYaeiouyCcLlNnRrSsZz", Some(Mark::Acute), "acute"),
    ("ÂÊÎÔÛâêîôûĈĉĜĝĤĥĴĵŜŝŴŵŶŷ", "AEIOUaeiouCcGgHhJjSsWwYy", Some(Mark::Circumflex), "circumflex"),
    ("ÃÑÕãñõĨĩŨũ", "ANOanoIiUu", Some(Mark::Tilde), "tilde"),
    ("ÄËÏÖÜäëïöüÿŸ", "AEIOUaeiouyY", Some(Mark::Diaeresis), "diaeresis"),
    ("ÅåŮů", "AaUu", Some(Mark::Ring), "ring"),
    ("ÇçĢģĶķĻļŅņŖŗŞşŢţ", "CcGgKkLlNnRrSsTt", Some(Mark::Cedilla), "cedilla"),
    ("ĀāĒēĪīŌōŪū", "AaEeIiOoUu", None, "macron"),
    ("ĂăĔĕĞğĬĭŎŏŬŭ", "AaEeGgIiOoUu", None, "breve"),
    ("ĊċĖėĠġİŻż", "CcEeGgIZz", None, "dot above"),
    ("ŐőŰű", "OoUu", None, "double acute"),
    ("ČčĎďĚěĽľŇňŘřŠšŤťŽž", "CcDdEeLlNnRrSsTtZz", None, "caron"),
    ("ĄąĘęĮįŲų", "AaEeIiUu", None, "ogonek"),
];

/// A letter as an ASCII letter and a mark: (base, the mark when the European font has it, its
/// name).
pub fn decomposed(c: char) -> Option<(char, Option<Mark>, &'static str)> {
    DECOMPOSITIONS.iter().find_map(|(letters, bases, mark, name)| {
        let at = letters.chars().position(|known| known == c)?;
        Some((bases.chars().nth(at)?, *mark, *name))
    })
}

/// Whether the build can draw a letter into the English font: one of the European font's own, or
/// an ASCII letter with a mark one of them has.
pub fn drawable(c: char) -> bool {
    european_glyph(c).is_some() || decomposed(c).is_some_and(|(_, mark, _)| mark.is_some())
}

/// Why the build cannot draw a character into the English font.
pub fn why_not_drawable(c: char) -> String {
    let marks = "grave, acute, circumflex, tilde, diaeresis, ring and cedilla";
    match decomposed(c) {
        Some((base, None, name)) => {
            format!("{c} is {base} with a {name}, a mark no letter of the European font has to copy (they have the {marks})")
        }
        _ => format!(
            "the build draws the European font's letters and ASCII letters with a mark one of them has (the {marks}), and {c:?} is neither"
        ),
    }
}

/// Every letter the build can draw into the English font: ENGLISH_EXTRA first, then the European
/// font's other letters, then the composed ones in code order.
pub fn drawable_letters() -> Vec<char> {
    let mut out: Vec<char> = ENGLISH_EXTRA.chars().collect();
    for byte in EUROPEAN_LETTERS {
        let c = latin_char(byte);
        if !out.contains(&c) {
            out.push(c);
        }
    }
    let mut composed: Vec<char> = DECOMPOSITIONS
        .iter()
        .filter(|(_, _, mark, _)| mark.is_some())
        .flat_map(|(letters, ..)| letters.chars())
        .filter(|c| !out.contains(c))
        .collect();
    composed.sort_unstable();
    out.extend(composed);
    out
}

/// A letter the English font gains spelled for a `comdatmes` name, which holds Shift-JIS: its base
/// letter (ŷ is y), or its ASCII spelling (ß ss); None for a character that is not one.
pub fn folded(c: char) -> Option<&'static str> {
    const ASCII: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    if let Some((base, _, _)) = decomposed(c) {
        let at = ASCII.find(base)?;
        return Some(&ASCII[at..at + 1]);
    }
    Some(match c {
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        'ª' => "a",
        'º' => "o",
        '°' => "o",
        '¡' => "!",
        '¿' => "?",
        _ => return None,
    })
}

/// The 32 letters glyphs 288 to 319 draw for a build whose English text uses `used` past the
/// font's own: each of ENGLISH_EXTRA keeps its glyph, and the other letters used take the glyphs of
/// the ENGLISH_EXTRA letters not used, in code order, so the same letters always get the same
/// glyphs and text using only ENGLISH_EXTRA builds as it always has. Refused past 32 letters, and
/// for a letter the build cannot draw.
pub fn english_slots(used: &BTreeSet<char>) -> Result<Vec<char>> {
    if let Some(&c) = used.iter().find(|&&c| !drawable(c)) {
        bail!("the English font has no {c:?} (U+{:04X}): {}", c as u32, why_not_drawable(c));
    }
    if used.len() > ENGLISH_EXTRA_COUNT {
        let all: String = used.iter().collect();
        bail!(
            "the English text uses {} letters past the English font's own, and its texture has room for {ENGLISH_EXTRA_COUNT} (glyphs 288 to 319): {all}",
            used.len()
        );
    }
    let mut slots: Vec<char> = ENGLISH_EXTRA.chars().collect();
    let free: Vec<usize> = (0..slots.len()).filter(|&at| !used.contains(&slots[at])).collect();
    let others = used.iter().filter(|c| !ENGLISH_EXTRA.contains(**c));
    for (&c, at) in others.zip(free) {
        slots[at] = c;
    }
    Ok(slots)
}

/// The characters of a table like fonttbl_1.bin (4 words, then each glyph's big-endian code),
/// for its first `count` glyphs: ASCII given as the character and a space, Hangul syllables (the
/// Korean table) as their Unicode code, anything else as Shift-JIS.
pub fn table_chars(table: &[u8], count: usize) -> Vec<Option<char>> {
    if table.len() < 8 {
        return Vec::new();
    }
    let count = ((table.len() - 8) / 2).min(count);
    (0..count)
        .map(|at| {
            let code = [table[8 + 2 * at], table[9 + 2 * at]];
            if code[1] == b' ' && code[0] < 0x80 {
                return Some(code[0] as char);
            }
            let wide = u16::from_be_bytes(code) as u32;
            if (0xAC00..=0xD7A3).contains(&wide) {
                return char::from_u32(wide);
            }
            let text = dc2tk_disc::text::decode_strict(&code)?;
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        })
        .collect()
}

/// A 4-bit font page of a TIM2 file: one picture, 16 colours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The bytes before the picture's pixels (file and picture headers).
    pub head: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// One palette index a pixel, row by row.
    pub pixels: Vec<u8>,
    /// The palette's alpha values (0 to 128), and its bytes as the file has them.
    pub alphas: Vec<u8>,
    pub clut: Vec<u8>,
}

fn u16_at(data: &[u8], at: usize) -> usize {
    u16::from_le_bytes([data[at], data[at + 1]]) as usize
}

fn u32_at(data: &[u8], at: usize) -> usize {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as usize
}

impl Page {
    pub fn read(data: &[u8]) -> Result<Page> {
        if data.len() < 0x40 || &data[..4] != b"TIM2" || u16_at(data, 6) != 1 {
            bail!("a font page is a TIM2 file of one picture");
        }
        let (clut_size, image_size, header_size) = (u32_at(data, 20), u32_at(data, 24), u16_at(data, 28));
        let (colours, image_type) = (u16_at(data, 30), data[35]);
        let (width, height) = (u16_at(data, 36), u16_at(data, 38));
        let start = 16 + header_size;
        if image_type != 4
            || colours != 16
            || image_size != width * height / 2
            || start + image_size + clut_size > data.len()
        {
            bail!("a font page is a 4-bit picture with 16 colours");
        }
        let pixels = data[start..start + image_size].iter().flat_map(|&b| [b & 15, b >> 4]).collect();
        let clut = data[start + image_size..start + image_size + clut_size].to_vec();
        let alphas = clut.chunks(4).map(|colour| colour[3]).collect();
        Ok(Page { head: data[..start].to_vec(), width, height, pixels, alphas, clut })
    }

    pub fn write(&self) -> Vec<u8> {
        let image: Vec<u8> = self.pixels.chunks(2).map(|pair| pair[0] | (pair[1] << 4)).collect();
        let mut out = self.head.clone();
        let total = (out.len() - 16) + image.len() + self.clut.len();
        out[16..20].copy_from_slice(&(total as u32).to_le_bytes());
        out[24..28].copy_from_slice(&(image.len() as u32).to_le_bytes());
        out[38..40].copy_from_slice(&(self.height as u16).to_le_bytes());
        out.extend(image);
        out.extend(&self.clut);
        out
    }

    /// The cells the page holds: whole rows of 32 glyphs.
    pub fn glyphs(&self) -> usize {
        self.height / 20 * (self.width / 16)
    }

    /// A glyph's cell as alpha values (0 to 128), 16 by 20.
    pub fn cell(&self, glyph: usize) -> Vec<u8> {
        let (x0, y0) = (glyph % 32 * 16, glyph / 32 * 20);
        let mut out = Vec::with_capacity(320);
        for y in y0..y0 + 20 {
            for x in x0..x0 + 16 {
                let index = if y < self.height { self.pixels[y * self.width + x] } else { 0 };
                out.push(self.alphas[index as usize]);
            }
        }
        out
    }

    /// Whether a glyph's cell has any ink.
    pub fn inked(&self, glyph: usize) -> bool {
        self.cell(glyph).iter().any(|&alpha| alpha > 0)
    }
}

/// The glyph of an ASCII character in every font (printable ASCII from the space, without the
/// backslash).
fn ascii_glyph(c: char) -> Option<usize> {
    let byte = u8::try_from(c as u32).ok().filter(|b| (0x20..0x7F).contains(b) && *b != b'\\')?;
    Some((byte - 0x20) as usize - usize::from(byte > b'\\'))
}

/// The first and last rows of a 16 by 20 cell with any ink.
fn ink_rows(cell: &[u8]) -> Option<(usize, usize)> {
    let inked: Vec<usize> = (0..20).filter(|&y| cell[y * 16..y * 16 + 16].iter().any(|&a| a > 0)).collect();
    Some((*inked.first()?, *inked.last()?))
}

/// The first and last columns of a cell with any ink.
fn ink_columns(cell: &[u8]) -> Option<(usize, usize)> {
    let inked: Vec<usize> = (0..16).filter(|&x| (0..20).any(|y| cell[y * 16 + x] > 0)).collect();
    Some((*inked.first()?, *inked.last()?))
}

/// An i or j without its dot (the ink above the first gap between rows of strong ink), for a mark
/// to go over.
fn dotless(cell: &[u8]) -> Vec<u8> {
    let strong = |y: usize| cell[y * 16..y * 16 + 16].iter().any(|&a| a >= 26);
    let mut out = cell.to_vec();
    let Some(dot) = (0..20).find(|&y| strong(y)) else {
        return out;
    };
    let Some(gap) = (dot..20).find(|&y| !strong(y)) else {
        return out;
    };
    if let Some(stem) = (gap..20).find(|&y| strong(y)) {
        out[..stem * 16].fill(0);
    }
    out
}

/// A letter's picture in the European font's shades, 16 by 20: the font's own, or an ASCII letter
/// of it with a mark copied off one of its letters. The mark is the ink of that letter above its
/// base letter's first row (below its last, for the cedilla), set as far from the new base letter
/// and as far off its middle; the small letter's mark first for a small letter, the capital's for
/// a capital, the other when that does not fit in the cell.
pub fn letter_cell(source: &Page, c: char) -> Result<Vec<u8>> {
    if let Some(glyph) = european_glyph(c) {
        return Ok(source.cell(glyph as usize));
    }
    let Some((base, Some(mark), _)) = decomposed(c) else {
        bail!("the English font has no {c:?} (U+{:04X}): {}", c as u32, why_not_drawable(c));
    };
    let Some(base_glyph) = ascii_glyph(base) else {
        bail!("{c}: {base} is not ASCII");
    };
    let mut cell = source.cell(base_glyph);
    if matches!(base, 'i' | 'j') && !mark.below() {
        cell = dotless(&cell);
    }
    let (Some((top, bottom)), Some((left, right))) = (ink_rows(&cell), ink_columns(&cell)) else {
        bail!("{c}: the European font's {base} has no ink");
    };
    let mut sources = mark.sources();
    if base.is_uppercase() {
        sources.reverse();
    }
    for (letter, letter_base) in sources {
        let (Some(from), Some(from_base)) = (european_glyph(letter), ascii_glyph(letter_base)) else {
            continue;
        };
        let (accented, plain) = (source.cell(from as usize), source.cell(from_base));
        let (Some((plain_top, plain_bottom)), Some((plain_left, plain_right)), Some((accented_top, accented_bottom))) =
            (ink_rows(&plain), ink_columns(&plain), ink_rows(&accented))
        else {
            continue;
        };
        // where the base letter is in the accented one (å's a is a row lower than a): its far
        // edge from the mark lines up with the plain letter's
        let (base_top, base_bottom) = if mark.below() {
            (accented_top, accented_top + plain_bottom - plain_top)
        } else {
            ((accented_bottom + plain_top).saturating_sub(plain_bottom), accented_bottom)
        };
        // the mark: the accented letter's ink beyond its base letter's rows
        let mut alone = vec![0u8; 320];
        for y in 0..20 {
            if (mark.below() && y > base_bottom) || (!mark.below() && y < base_top) {
                alone[y * 16..y * 16 + 16].copy_from_slice(&accented[y * 16..y * 16 + 16]);
            }
        }
        let (Some((mark_top, mark_bottom)), Some((mark_left, mark_right))) = (ink_rows(&alone), ink_columns(&alone))
        else {
            continue;
        };
        // as far from the base letter, and off its middle, as on the letter it came from
        let dy = if mark.below() { bottom as i64 - base_bottom as i64 } else { top as i64 - base_top as i64 };
        let off = (mark_left + mark_right) as i64 - (plain_left + plain_right) as i64;
        let dx = ((left + right) as i64 + off - (mark_left + mark_right) as i64).div_euclid(2);
        let fits = |low: usize, high: usize, by: i64, size: i64| low as i64 + by >= 0 && high as i64 + by < size;
        if !fits(mark_top, mark_bottom, dy, 20) || !fits(mark_left, mark_right, dx, 16) {
            continue;
        }
        let mut out = cell.clone();
        for y in mark_top..=mark_bottom {
            for x in mark_left..=mark_right {
                let to = (y as i64 + dy) as usize * 16 + (x as i64 + dx) as usize;
                out[to] = out[to].max(alone[y * 16 + x]);
            }
        }
        return Ok(out);
    }
    bail!(
        "the English font has no {c:?} (U+{:04X}): the {} does not fit {} {base} in a glyph's 16 by 20 pixels",
        c as u32,
        mark.name(),
        if mark.below() { "under" } else { "over" }
    )
}

/// The English font texture with `letters` (english_slots: ENGLISH_EXTRA, or the letters a build's
/// text uses) drawn as glyphs 288 on, from the European font's pictures (letter_cell): the page
/// grows by the rows the 10th row of glyphs needs (200 pixels tall, within the game's
/// 0xD000-byte buffer), each letter centred in its cell since a glyph past the ASCII ones advances
/// the pen a whole cell (CheckHalfFont, font.cpp), and its shades mapped to the English palette's
/// nearest.
pub fn extended_english(english: &[u8], european: &[u8], letters: &[char]) -> Result<Vec<u8>> {
    let mut page = Page::read(english)?;
    let source = Page::read(european)?;
    if letters.len() > ENGLISH_EXTRA_COUNT {
        bail!("the English font has room for {ENGLISH_EXTRA_COUNT} more glyphs, not {}", letters.len());
    }
    let last = ENGLISH_EXTRA_FIRST as usize + letters.len();
    let height = last.div_ceil(32) * 20;
    if page.height < height {
        page.pixels.resize(page.width * height, 0);
        page.height = height;
    }
    for (at, &c) in letters.iter().enumerate() {
        let cell = letter_cell(&source, c)?;
        let (left, right) = ink_columns(&cell).unwrap_or((0, 0));
        // the ink's middle at the cell's (a whole cell is 15 pixels of pen)
        let shift = 7 - (left + right) as i64 / 2;
        let glyph = ENGLISH_EXTRA_FIRST as usize + at;
        let (x0, y0) = (glyph % 32 * 16, glyph / 32 * 20);
        for y in 0..20 {
            for x in 0..16i64 {
                let from_x = x - shift;
                let alpha = if (0..16).contains(&from_x) { cell[y * 16 + from_x as usize] } else { 0 };
                let index = (0..page.alphas.len())
                    .min_by_key(|&i| (page.alphas[i] as i64 - alpha as i64).abs())
                    .unwrap_or(0) as u8;
                page.pixels[(y0 + y) * page.width + x0 + x as usize] = index;
            }
        }
    }
    let out = page.write();
    if out.len() > FONT_BUFFER {
        bail!("the English font texture would take {} bytes; the game loads it into {FONT_BUFFER}", out.len());
    }
    Ok(out)
}

/// Whether a message file uses the glyphs the English font gains (288 on), so the build must give
/// the game the extended texture.
pub fn uses_english_extra(codes: impl IntoIterator<Item = u16>) -> bool {
    let end = ENGLISH_EXTRA_FIRST + ENGLISH_EXTRA.chars().count() as u16;
    codes.into_iter().any(|code| (ENGLISH_EXTRA_FIRST..end).contains(&code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_japanese_numbering_is_in_shift_jis_order_then_unicode_order() {
        let chars: Vec<char> = JAPANESE.chars().collect();
        assert_eq!(chars.len() + 94, JAPANESE_GLYPHS);
        assert_eq!((chars[0], chars[1], chars[1921 - 94]), ('\u{3000}', '、', '腕'));
        let sjis = |c: char| dc2tk_disc::text::encode(&c.to_string()).map(|b| u16::from_be_bytes([b[0], b[1]]));
        for pair in chars[..1921 - 94 + 1].windows(2) {
            assert!(sjis(pair[0]) < sjis(pair[1]), "{} {}", pair[0], pair[1]);
        }
        let tail = &chars[1922 - 94..];
        assert!(tail.windows(2).all(|pair| pair[0] < pair[1]));
        let mut unique = chars.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), chars.len());
    }

    #[test]
    fn tables_read_ascii_shift_jis_and_hangul() {
        let mut table = vec![0u8; 8];
        table.extend([b'A', b' ', 0x81, 0x40, 0xAC, 0x00, 0x88, 0x9F]);
        assert_eq!(table_chars(&table, 10), vec![Some('A'), Some('\u{3000}'), Some('가'), Some('亜')]);
        assert_eq!(table_chars(&table, 2).len(), 2);
    }

    fn page(height: usize, alphas: &[u8]) -> Vec<u8> {
        let mut out = b"TIM2\x04\x00\x01\x00".to_vec();
        out.extend([0; 8]);
        let image = 512 * height / 2;
        out.extend(((0x30 + image + 64) as u32).to_le_bytes());
        out.extend(64u32.to_le_bytes());
        out.extend((image as u32).to_le_bytes());
        out.extend(0x30u16.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend([0, 1, 3, 4]);
        out.extend(512u16.to_le_bytes());
        out.extend((height as u16).to_le_bytes());
        out.extend([0; 24]);
        out.extend(vec![0; image]);
        for alpha in alphas {
            out.extend([255, 255, 255, *alpha]);
        }
        out
    }

    #[test]
    fn the_english_page_gains_a_row_of_accented_letters() {
        let english: Vec<u8> = (0..16).map(|i| (i * 128 / 15) as u8).collect();
        let european: Vec<u8> = (0..16).map(|i| (i * 127 / 15) as u8).collect();
        let mut source = page(192, &european);
        // the European glyph of é (0x5E + 42) as a block of colour 15
        let glyph = european_glyph('é').unwrap() as usize;
        let start = 16 + 0x30;
        for y in 4..10 {
            for x in 2..6 {
                let (px, py) = (glyph % 32 * 16 + x, glyph / 32 * 20 + y);
                let at = start + (py * 512 + px) / 2;
                source[at] = if px % 2 == 0 { (source[at] & 0xF0) | 15 } else { (source[at] & 0x0F) | 0xF0 };
            }
        }
        let built =
            extended_english(&page(192, &english), &source, &ENGLISH_EXTRA.chars().collect::<Vec<_>>()).unwrap();
        let read = Page::read(&built).unwrap();
        assert_eq!((read.height, read.glyphs(), built.len()), (200, 320, 16 + 0x30 + 512 * 100 + 64));
        let at = ENGLISH_EXTRA_FIRST as usize + ENGLISH_EXTRA.chars().position(|c| c == 'é').unwrap();
        let cell = read.cell(at);
        // centred: ink in columns 6 to 9 (it was at 2 to 5)
        let columns: Vec<usize> = (0..16).filter(|&x| (0..20).any(|y| cell[y * 16 + x] == 128)).collect();
        assert_eq!(columns, vec![6, 7, 8, 9]);
        // the other glyphs keep their pixels
        assert_eq!(read.cell(5), Page::read(&page(192, &english)).unwrap().cell(5));
        assert!(extended_english(&page(300, &english), &source, &['é']).unwrap_err().to_string().contains("53248"));
        assert!(uses_english_extra([1, 2, 300]) && !uses_english_extra([1, 287, 320]));
    }

    /// Paints a glyph's cell of a made-up page: each (x, y, colour).
    fn paint(data: &mut [u8], glyph: usize, pixels: &[(usize, usize, u8)]) {
        for &(x, y, colour) in pixels {
            let (px, py) = (glyph % 32 * 16 + x, glyph / 32 * 20 + y);
            let at = 16 + 0x30 + (py * 512 + px) / 2;
            data[at] = if px % 2 == 0 { (data[at] & 0xF0) | colour } else { (data[at] & 0x0F) | (colour << 4) };
        }
    }

    fn block(x: std::ops::Range<usize>, y: std::ops::Range<usize>) -> Vec<(usize, usize, u8)> {
        y.flat_map(|y| x.clone().map(move |x| (x, y, 15))).collect()
    }

    #[test]
    fn letters_no_font_has_are_composed_of_a_letter_and_a_mark() {
        let alphas: Vec<u8> = (0..16).map(|i| (i * 127 / 15) as u8).collect();
        let mut source = page(192, &alphas);
        // e: rows 8 to 15; ê: a lower e under a circumflex (rows 4 and 5); y: rows 8 to 18
        let e = block(1..7, 8..16);
        paint(&mut source, ascii_glyph('e').unwrap(), &e);
        let mut hat = block(2..6, 4..5);
        hat.extend([(1, 5, 15), (6, 5, 15)]);
        let lower: Vec<(usize, usize, u8)> = e.iter().map(|&(x, y, c)| (x, y + 1, c)).collect();
        paint(&mut source, european_glyph('ê').unwrap() as usize, &[hat.clone(), lower].concat());
        let y = block(3..9, 8..19);
        paint(&mut source, ascii_glyph('y').unwrap(), &y);
        let read = Page::read(&source).unwrap();
        let cell = letter_cell(&read, 'ŷ').unwrap();
        // the y, and the circumflex as far above it (the e is a row lower in ê: two rows up from
        // its top), centred over it as over the e (the y's middle is 2 pixels right of the e's)
        let mut wanted = vec![0u8; 320];
        for &(x, row, _) in &y {
            wanted[row * 16 + x] = 127;
        }
        for &(x, row, _) in &hat {
            wanted[(row - 1) * 16 + x + 2] = 127;
        }
        assert_eq!(cell, wanted);
        // a letter of the European font is its own picture
        assert_eq!(letter_cell(&read, 'ê').unwrap(), read.cell(european_glyph('ê').unwrap() as usize));
        // a mark the font has no letter with, and a mark that does not fit
        let error = letter_cell(&read, 'č').unwrap_err().to_string();
        assert!(error.contains("č is c with a caron, a mark no letter of the European font has"), "{error}");
        paint(&mut source, ascii_glyph('g').unwrap(), &block(2..7, 6..20));
        paint(&mut source, ascii_glyph('c').unwrap(), &block(2..7, 6..14));
        paint(&mut source, european_glyph('ç').unwrap() as usize, &[block(2..7, 6..14), block(3..5, 15..18)].concat());
        let read = Page::read(&source).unwrap();
        let error = letter_cell(&read, 'ģ').unwrap_err().to_string();
        assert!(error.contains("the cedilla does not fit under g"), "{error}");
        assert_eq!(letter_cell(&read, 'ç').unwrap(), read.cell(european_glyph('ç').unwrap() as usize));
        // the composed letter drawn into the English page, as the slots give it
        let english: Vec<u8> = (0..16).map(|i| (i * 128 / 15) as u8).collect();
        let slots = english_slots(&['ŷ', 'é'].into_iter().collect()).unwrap();
        let built = Page::read(&extended_english(&page(192, &english), &source, &slots).unwrap()).unwrap();
        let at = ENGLISH_EXTRA_FIRST as usize + slots.iter().position(|&c| c == 'ŷ').unwrap();
        let drawn: Vec<bool> = built.cell(at).iter().map(|&alpha| alpha > 0).collect();
        // centred: the y's columns 3 to 8 at 5 to 10 (the ink's middle at 7)
        let shifted: Vec<bool> = (0..320).map(|i| i % 16 >= 2 && wanted[i - 2] > 0).collect();
        assert_eq!(drawn, shifted);
    }

    #[test]
    fn the_extra_glyphs_go_to_the_letters_used() {
        let set = |text: &str| text.chars().collect::<BTreeSet<char>>();
        let extra: Vec<char> = ENGLISH_EXTRA.chars().collect();
        // ENGLISH_EXTRA's letters keep their glyphs: text using only them builds as it always has
        assert_eq!(english_slots(&set("éü¡")).unwrap(), extra);
        assert_eq!(english_slots(&set("")).unwrap(), extra);
        // others take the glyphs of the ones not used, in code order, whatever order they come in
        let slots = english_slots(&set("ŷé¡ĉ")).unwrap();
        assert_eq!((slots[0], slots[1], slots[2], slots[18]), ('¡', 'ĉ', 'ŷ', 'é'));
        assert_eq!(slots[3..].iter().filter(|c| !extra.contains(c)).count(), 0);
        // any 32 letters
        let many = set("ŷŶĉĈĝĜĥĤĵĴŝŜŵŴĩĨũŨůŮĺĹńŃŕŔśŚźŹćĆ");
        assert_eq!(many.len(), 32);
        let slots = english_slots(&many).unwrap();
        assert_eq!(slots.iter().copied().collect::<BTreeSet<char>>(), many);
        assert_eq!(slots[0], 'Ć');
        // not 33, nor a letter the build cannot draw
        let error = english_slots(&set("ŷŶĉĈĝĜĥĤĵĴŝŜŵŴĩĨũŨůŮĺĹńŃŕŔśŚźŹćĆé")).unwrap_err().to_string();
        assert!(error.contains("uses 33 letters past the English font's own"), "{error}");
        assert!(error.contains("éĆćĈĉ"), "{error}");
        let error = english_slots(&set("ŷš")).unwrap_err().to_string();
        assert!(error.contains("'š' (U+0161): š is s with a caron"), "{error}");
        assert!(english_slots(&set("€")).unwrap_err().to_string().contains("is neither"));
        // what can be drawn, and how a name in Shift-JIS spells it
        let letters = drawable_letters();
        assert_eq!(&letters[..32], &extra[..]);
        assert!(letters.contains(&'ŷ') && letters.contains(&'Œ') && !letters.contains(&'š') && !letters.contains(&'Ø'));
        assert_eq!((folded('ŷ'), folded('ß'), folded('Ç'), folded('Ø')), (Some("y"), Some("ss"), Some("C"), None));
    }

    /// Every letter the build can draw, from the European font of an extracted disc (C2S_CORPUS):
    /// the composed ones drawn, shown with C2S_SHOW_LETTERS set.
    #[test]
    fn corpus_letters_compose() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let root = std::path::Path::new(&corpus);
        let source = Page::read(&std::fs::read(root.join(EUROPEAN_TEXTURE)).unwrap()).unwrap();
        let mut refused = String::new();
        for c in drawable_letters() {
            match letter_cell(&source, c) {
                Ok(cell) if std::env::var_os("C2S_SHOW_LETTERS").is_some() => {
                    eprintln!("{c}");
                    for row in cell.chunks(16) {
                        eprintln!(
                            "|{}|",
                            row.iter().map(|&a| b" .:-=+*#%@"[a as usize * 9 / 128] as char).collect::<String>()
                        );
                    }
                }
                Ok(_) => {}
                Err(_) => refused.push(c),
            }
        }
        // a cedilla under g's descender does not fit
        assert_eq!(refused, "ģ");
    }
}
