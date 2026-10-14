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

/// Accented letters added to the English font as glyphs 288 to 319 (ENGLISH_EXTRA_FIRST on), in
/// this order, the ones the French, German, Italian and Spanish text uses most. Their pictures are
/// the European font's own, copied in at build time. The English texture is loaded into a buffer
/// of 0xD000 bytes (font_buff, mainloop.cpp), which leaves room for one more row of 32 glyphs and
/// no more.
pub const ENGLISH_EXTRA: &str = "¡¿ÀÁÄÇÈÉÑÖÜßàáâäçèéêëíîïñóôöùúûü";
pub const ENGLISH_EXTRA_FIRST: u16 = 288;
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
            let text = c2_disc::text::decode_strict(&code)?;
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

/// The English font texture with the accented letters of ENGLISH_EXTRA drawn as glyphs 288 to
/// 319, from the European font's pictures: the page grows by the rows the 10th row of glyphs
/// needs (200 pixels tall, within the game's 0xD000-byte buffer), each letter centred in its cell
/// since a glyph past the ASCII ones advances the pen a whole cell (CheckHalfFont, font.cpp), and
/// its shades mapped to the English palette's nearest. An English texture already holding them is
/// given back as it is.
pub fn extended_english(english: &[u8], european: &[u8]) -> Result<Vec<u8>> {
    let mut page = Page::read(english)?;
    let source = Page::read(european)?;
    let last = ENGLISH_EXTRA_FIRST as usize + ENGLISH_EXTRA.chars().count();
    let height = last.div_ceil(32) * 20;
    if page.height < height {
        page.pixels.resize(page.width * height, 0);
        page.height = height;
    }
    for (at, c) in ENGLISH_EXTRA.chars().enumerate() {
        let Some(from) = european_glyph(c) else {
            bail!("the European font has no {c}");
        };
        let cell = source.cell(from as usize);
        let inked: Vec<usize> = (0..16).filter(|&x| (0..20).any(|y| cell[y * 16 + x] > 0)).collect();
        let (left, right) = (inked.first().copied().unwrap_or(0), inked.last().copied().unwrap_or(0));
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
        let sjis = |c: char| c2_disc::text::encode(&c.to_string()).map(|b| u16::from_be_bytes([b[0], b[1]]));
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
        let built = extended_english(&page(192, &english), &source).unwrap();
        let read = Page::read(&built).unwrap();
        assert_eq!((read.height, read.glyphs(), built.len()), (200, 320, 16 + 0x30 + 512 * 100 + 64));
        let at = ENGLISH_EXTRA_FIRST as usize + ENGLISH_EXTRA.chars().position(|c| c == 'é').unwrap();
        let cell = read.cell(at);
        // centred: ink in columns 6 to 9 (it was at 2 to 5)
        let columns: Vec<usize> = (0..16).filter(|&x| (0..20).any(|y| cell[y * 16 + x] == 128)).collect();
        assert_eq!(columns, vec![6, 7, 8, 9]);
        // the other glyphs keep their pixels
        assert_eq!(read.cell(5), Page::read(&page(192, &english)).unwrap().cell(5));
        assert!(extended_english(&page(300, &english), &source).unwrap_err().to_string().contains("53248"));
        assert!(uses_english_extra([1, 2, 300]) && !uses_english_extra([1, 287, 320]));
    }
}
