//! A map's scene file (`.map`, docs/assets/maps.md sections 2-5) as a typed tree, read and written
//! back byte for byte.
//!
//! The file is a tag script (`KEYWORD a,b,...;`, cp932 text) that the engine reads in two passes:
//! textures, packs and settings, then templates, placements, cameras and trigger points. The tree
//! keeps every value as the engine reads it and what the writer needs to give the file back
//! exactly: which optional lines are there and how many arguments they have, and each number as
//! the file types it ([`Num`]). The layout (indentation, blank lines, the spaces after some commas,
//! the counts on `FUNC_POINT`, `CAMERA_INFO` and `PIECE_MATERIAL_START`) is computed, as the game's
//! exporter wrote it (section 2.3), so an edited tree writes a file in the same style.

use serde::{Deserialize, Serialize};

use crate::{bail, Error, Result};

// ---------- numbers ----------

/// A number argument as the file types it, which is how the engine reads it: text with a `.` is a
/// float, other text an int, and text with a letter (the exporter's exponents, `8.4208e-007`) is
/// not a number to the engine, which reads it as 0. Ints and floats convert freely where the engine
/// wants the other.
///
/// Written as C's `%g` (6 significant digits): a float that is whole is written without a dot, so
/// it reads back as an int. Build new numbers with `Num::from`, which gives what the file will
/// hold. Floats are never written with an exponent (tiny ones as plain decimals); only
/// `Exponent`, which keeps an exponent the file had, is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Num {
    Int(i32),
    Float(f32),
    /// Text with an exponent: the value written, which the engine reads as 0.
    Exponent {
        exponent: f32,
    },
}

impl Num {
    /// The value as the engine reads it into a float.
    pub fn float(self) -> f32 {
        match self {
            Num::Int(value) => value as f32,
            Num::Float(value) => value,
            Num::Exponent { .. } => 0.0,
        }
    }

    /// The value as the engine reads it into an int (a float is truncated).
    pub fn int(self) -> i32 {
        match self {
            Num::Int(value) => value,
            Num::Float(value) => value as i32,
            Num::Exponent { .. } => 0,
        }
    }

    /// The number's text in the file.
    pub fn text(self) -> String {
        match self {
            Num::Int(value) => value.to_string(),
            Num::Float(value) => {
                let (negative, digits, exponent) = significant(value as f64);
                signed(negative, positional(&digits, exponent))
            }
            Num::Exponent { exponent: value } => {
                let (negative, digits, exponent) = significant(value as f64);
                let body = if !(-4..6).contains(&exponent) {
                    let rest = digits[1..].trim_end_matches('0');
                    let mantissa =
                        if rest.is_empty() { digits[..1].to_string() } else { format!("{}.{rest}", &digits[..1]) };
                    let sign = if exponent < 0 { '-' } else { '+' };
                    format!("{mantissa}e{sign}{:03}", exponent.unsigned_abs())
                } else {
                    positional(&digits, exponent)
                };
                signed(negative, body)
            }
        }
    }

    /// The number a text is, as the engine types it; None when it is not one.
    pub fn parse(text: &str) -> Option<Num> {
        if text.is_empty() {
            return None;
        }
        if text.bytes().any(|b| b.is_ascii_alphabetic()) {
            return text.parse().ok().map(|exponent| Num::Exponent { exponent });
        }
        if text.contains('.') {
            return text.parse().ok().map(Num::Float);
        }
        text.parse().ok().map(Num::Int)
    }
}

impl Default for Num {
    fn default() -> Num {
        Num::Int(0)
    }
}

impl From<i32> for Num {
    fn from(value: i32) -> Num {
        Num::Int(value)
    }
}

/// The number the file holds once the value is written: rounded to 6 significant digits, an int
/// when that is whole.
impl From<f32> for Num {
    fn from(value: f32) -> Num {
        if !value.is_finite() {
            return Num::Int(0);
        }
        Num::parse(&Num::Float(value).text()).unwrap_or_default()
    }
}

/// (negative, 6 significant digits, decimal exponent) of a value, as `%.5e` rounds it.
fn significant(value: f64) -> (bool, String, i32) {
    if value == 0.0 || !value.is_finite() {
        return (false, "000000".into(), 0);
    }
    let text = format!("{:.5e}", value.abs());
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    (value < 0.0, mantissa.replace('.', ""), exponent.parse().unwrap_or(0))
}

/// Digits with the point after `exponent + 1` of them, without trailing zeros after the point.
fn positional(digits: &str, exponent: i32) -> String {
    let point = exponent + 1;
    let text = if point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}", "0".repeat(point as usize - digits.len()))
    } else {
        format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
    };
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// A sign before a body, but never `-0`.
fn signed(negative: bool, body: String) -> String {
    if negative && body != "0" {
        format!("-{body}")
    } else {
        body
    }
}

// ---------- the tree ----------

/// A whole .map file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapFile {
    /// `IMG`: texture files (`.img` in the map's `.ipk`).
    pub images: Vec<String>,
    /// `PCP`: model packs (`.pcp` in the map's `.mpk`).
    pub packs: Vec<String>,
    /// The order of the IMG and PCP lines when it is not all IMGs then all PCPs (one file, g00).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_order: Option<Vec<HeaderLine>>,
    /// `PARTS`: templates, drawn only where placed. Names can repeat (the engine uses the first).
    pub templates: Vec<Template>,
    pub settings: Settings,
    /// `MAP_PARTS`: placed copies of templates. Scripts address them by slot, so append new ones.
    pub placements: Vec<Placement>,
    /// `CAMERA_INFO`: fixed cameras.
    pub cameras: Option<Vec<FixCamera>>,
    /// The map's own trigger points (part `""` to scripts), after the placements.
    pub points: Option<Vec<Point>>,
}

/// Which list an IMG or PCP line comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HeaderLine {
    Img,
    Pcp,
}

/// `PARTS`: a template of pieces and trigger points.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    /// `FAR_CLIP distance,fade`.
    pub far_clip: Option<[Num; 2]>,
    /// `LIGHT_FLAG no_light,no_plight`.
    pub light_flag: [Num; 2],
    /// `MOVE_FLAG`: closed sides -X, -Z, +X, +Z of a dungeon cell. Missing in parts of an older
    /// exporter, which wrote their LIGHT_FLAG without indentation.
    pub move_flag: Option<[Num; 4]>,
    pub pieces: Vec<Piece>,
    /// `FUNC_POINT`: points every placement of the template carries.
    pub points: Option<Vec<Point>>,
}

/// `PIECE`: a model placed in a template.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    /// The model file (`.mds` or `.chr`), found by name in the map's packs.
    pub model: String,
    pub show: Num,
    /// `PIECE_NAME`.
    pub name: String,
    pub pos: [Num; 3],
    /// Radians.
    pub rot: [Num; 3],
    pub scale: [Num; 3],
    /// `PIECE_MATERIAL_START`: colour overrides of materials of frames.
    pub materials: Option<Vec<PieceMaterial>>,
    /// `PIECE_COL_TYPE`.
    pub col_type: Option<ColType>,
    /// `PIECE_TIME start_hour,end_hour`: equal is always.
    pub time: [Num; 2],
}

/// `PIECE_MATERIAL "frame",material,r,g,b,a,flag`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PieceMaterial {
    pub frame: String,
    pub material: Num,
    pub color: [Num; 4],
    pub flag: Num,
}

/// `PIECE_COL_TYPE type[,param]`: 0 is normal; other pieces are not copied into placements.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ColType {
    pub kind: Num,
    pub param: Option<Num>,
}

/// The settings pass's lines, between the templates and the placements.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub active_light_set: Num,
    /// `LIGHT_SET` blocks, with their numbers (two files skip some).
    pub light_sets: Vec<LightSet>,
    pub floor: Num,
    pub chara_pos: [Num; 3],
    pub def_foot: Num,
    pub time_flag: TimeFlag,
    pub time_light_num: Num,
    pub time_cfade: Option<Num>,
    pub sky_info: SkyInfo,
    pub lens_flare: Option<Num>,
    pub all_scissor: Option<Num>,
    /// `CHARA_LIGHT_ADJUST switch,a,b,c`.
    pub chara_light_adjust: Option<[Num; 4]>,
}

/// `TIME_FLAG follow_clock,blend[,fixed_time[,fixed_time_enable]]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TimeFlag {
    pub follow_clock: Num,
    pub blend: Num,
    pub fixed_time: Option<Num>,
    /// Written only after `fixed_time`.
    pub fixed_time_enable: Option<Num>,
}

/// `SKY_INFO sky,offset[,sun_angle_degrees]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SkyInfo {
    pub sky: Num,
    pub offset: Num,
    pub sun_angle: Option<Num>,
}

/// `LIGHT_SET n`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LightSet {
    pub index: Num,
    /// Ignored by the engine (the projection is always 52 degrees).
    pub fov: Num,
    pub bgcolor: [Num; 3],
    pub bgcolor2: [Num; 3],
    pub ambient: [Num; 3],
    /// `LIGHT` and `PLIGHT` lines in their order.
    pub lights: Vec<SetLight>,
    pub fog_enable: Num,
    pub fog: Fog,
}

/// A light of a light set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SetLight {
    Light(Light),
    Plight(PointLight),
}

/// `LIGHT index,dx,dy,dz[,r,g,b]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub index: Num,
    pub dir: [Num; 3],
    pub color: Option<[Num; 3]>,
}

/// `PLIGHT index,power,x,y,z,r,g,b`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PointLight {
    pub index: Num,
    pub power: Num,
    pub pos: [Num; 3],
    pub color: [Num; 3],
}

/// `FOG near,far[,r,g,b[,offset,far_value]]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Fog {
    pub near: Num,
    pub far: Num,
    pub color: Option<[Num; 3]>,
    /// Offset and far value, written only after the colour.
    pub offset_far: Option<[Num; 2]>,
}

/// `MAP_PARTS`: a placed copy of a template.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub name: String,
    pub show: Num,
    /// `MAP_FAR_CLIP distance,fade`.
    pub far_clip: Option<[Num; 2]>,
    /// `PARTS_NAME`: the template.
    pub template: String,
    /// `PARTS_GROUP`.
    pub group: String,
    pub pos: [Num; 3],
    /// Radians.
    pub rot: [Num; 3],
    pub scale: [Num; 3],
}

/// `FIX_CAMERA`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FixCamera {
    pub pos: [Num; 3],
    /// `FIX_CAMERA_POS2 index,x,y,z`.
    pub pos2: Vec<CameraPos>,
    /// `FIX_CAMERA_OFF_GROUP index,"group",value`.
    pub off_groups: Vec<CameraOffGroup>,
    /// `FIX_CAMERA_RECT`.
    pub rects: Vec<CameraRect>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraPos {
    pub index: Num,
    pub pos: [Num; 3],
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraOffGroup {
    pub index: Num,
    pub group: String,
    pub value: Num,
}

/// `FIX_CAMERA_RECT "box", min xyz, max xyz, pos, rot, scale`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CameraRect {
    pub name: String,
    pub min: [Num; 3],
    pub max: [Num; 3],
    pub pos: [Num; 3],
    pub rot: [Num; 3],
    pub scale: [Num; 3],
}

/// `FUNC_DATA`: a trigger point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub kind: PointKind,
    pub enable: Num,
    /// `FUNC_NAME`: how scripts find it.
    pub name: String,
    /// `FUNC_FLAG a, b, start_hour,end_hour`.
    pub flag: [Num; 4],
    /// The kind's data line; flares and fires share `FUNC_FIRE_DATA`, `pos` has none.
    pub data: PointData,
    /// `FUNC_POS`: position, rotation (radians), scale.
    pub pos: [Num; 3],
    pub rot: [Num; 3],
    pub scale: [Num; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    Pos,
    Invent,
    Event,
    Flare,
    Plight,
    Fire,
    Effect,
    Sound,
    Anime,
}

impl PointKind {
    pub const ALL: [PointKind; 9] = [
        PointKind::Pos,
        PointKind::Invent,
        PointKind::Event,
        PointKind::Flare,
        PointKind::Plight,
        PointKind::Fire,
        PointKind::Effect,
        PointKind::Sound,
        PointKind::Anime,
    ];

    /// The name in `FUNC_DATA`.
    pub fn name(self) -> &'static str {
        match self {
            PointKind::Pos => "pos",
            PointKind::Invent => "invent",
            PointKind::Event => "event",
            PointKind::Flare => "flare",
            PointKind::Plight => "plight",
            PointKind::Fire => "fire",
            PointKind::Effect => "effect",
            PointKind::Sound => "sound",
            PointKind::Anime => "anime",
        }
    }

    pub fn from_name(name: &str) -> Option<PointKind> {
        PointKind::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// A trigger point's data line.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointData {
    #[default]
    None,
    Invent(InventData),
    Event(EventData),
    Fire(FireData),
    Plight(PlightData),
    /// `FUNC_EFFECT_NAME`: an effect of the map's `.efp`.
    Effect(String),
    Sound(SoundData),
    Anime(AnimeData),
}

/// `FUNC_INVENT_DATA id, min xyz, max xyz, int, float, angle_degrees`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InventData {
    pub id: Num,
    pub min: [Num; 3],
    pub max: [Num; 3],
    pub int_param: Num,
    pub float_param: Num,
    pub angle: Num,
}

/// `FUNC_EVENT_DATA "kind",event_no,program,a,b,c,"target",action_button,use_item,flag_100`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventData {
    /// door, event, t_box, close_door, book, ed_door, lddr_b, lddr_t.
    pub kind: String,
    pub event_no: Num,
    /// The map script's PROGRAM it runs.
    pub program: Num,
    pub args: [Num; 3],
    /// Up to 15 characters (a map for doors).
    pub target: String,
    pub action_button: Num,
    pub use_item: Num,
    pub flag_100: Num,
}

/// `FUNC_FIRE_DATA r,g,b,draw[,a,b]` (flares and fires).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FireData {
    /// All zero means 128,90,38.
    pub color: [Num; 3],
    pub draw: Num,
    pub extra: Option<[Num; 2]>,
}

/// `FUNC_PLIGHT_DATA power, r,g,b, mode, 4 ints, flicker_type, flicker_depth, flicker_period`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlightData {
    pub power: Num,
    /// 0-511.
    pub color: [Num; 3],
    pub mode: Num,
    pub ints: [Num; 4],
    pub flicker_type: Num,
    pub flicker_depth: Num,
    pub flicker_period: Num,
}

/// `FUNC_SOUND_DATA number, near, far, int, shape, start xyz, end xyz`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SoundData {
    pub number: Num,
    pub near: Num,
    pub far: Num,
    pub int_param: Num,
    pub shape: Num,
    pub start: [Num; 3],
    pub end: [Num; 3],
}

/// `FUNC_ANIME_DATA "placement","piece","frame",drives,mode, start xyz, step xyz, end xyz[,uniform,relative]`
/// (`mapFUNC_ANIME_DATA`, mapload.cpp:1257): a value of a placement, piece or model node that
/// `CObjAnime::Step` changes every frame (docs/assets/maps.md section 5.1).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimeData {
    /// The placement a map's own point moves (`MAP_PARTS` name); a template's points move the
    /// placement that copies them and ignore it.
    pub placement: String,
    /// The `PIECE_NAME` moved; empty moves the whole placement.
    pub piece: String,
    /// The piece's model node moved (compared up to `--`); empty moves the whole piece.
    pub frame: String,
    /// 1 position, 2 rotation (degrees), 3 scale, 4 colour (unlit, 0-255), 5 alpha (0-1).
    #[serde(alias = "kind")]
    pub drives: Num,
    /// 0 hold, 1 loop, 2 stop at the end, 3 repeat, 4 ping-pong, 5 there and back once, 6 look
    /// at the player, 7 random, 8 minute hand, 9 hour hand, 10 by the hour.
    #[serde(alias = "int_param")]
    pub mode: Num,
    pub start: [Num; 3],
    /// Added every frame; in mode 10, start hour, end hour and fade hours.
    pub step: [Num; 3],
    pub end: [Num; 3],
    /// `[uniform, relative]` when the line has them: y and z follow x; a position is in the
    /// piece's own space (with no frame, the piece moves by it every frame).
    #[serde(alias = "extra")]
    pub flags: Option<[Num; 2]>,
}

// ---------- reading ----------

#[derive(Clone, Debug)]
enum Arg {
    Str(String),
    Num(Num),
}

#[derive(Debug)]
struct Stmt {
    keyword: String,
    args: Vec<Arg>,
    line: usize,
}

impl Stmt {
    fn error(&self, message: impl std::fmt::Display) -> Error {
        Error::invalid(format!("line {}: {}: {message}", self.line, self.keyword))
    }

    fn counts(&self, allowed: &[usize]) -> Result<()> {
        if allowed.contains(&self.args.len()) {
            Ok(())
        } else {
            Err(self.error(format!("{} arguments, expected {allowed:?}", self.args.len())))
        }
    }

    fn num(&self, at: usize) -> Result<Num> {
        match self.args.get(at) {
            Some(Arg::Num(value)) => Ok(*value),
            _ => Err(self.error(format!("argument {} is not a number", at + 1))),
        }
    }

    fn string(&self, at: usize) -> Result<String> {
        match self.args.get(at) {
            Some(Arg::Str(value)) => Ok(value.clone()),
            _ => Err(self.error(format!("argument {} is not a string", at + 1))),
        }
    }

    fn nums<const N: usize>(&self, from: usize) -> Result<[Num; N]> {
        let mut out = [Num::default(); N];
        for (at, slot) in out.iter_mut().enumerate() {
            *slot = self.num(from + at)?;
        }
        Ok(out)
    }

    /// N numbers from `from` when the line has them.
    fn optional<const N: usize>(&self, from: usize) -> Result<Option<[Num; N]>> {
        if self.args.len() >= from + N {
            self.nums(from).map(Some)
        } else {
            Ok(None)
        }
    }

    /// The only number of a one-argument line.
    fn one(&self) -> Result<Num> {
        self.counts(&[1])?;
        self.num(0)
    }

    /// N numbers, all the line has.
    fn exactly<const N: usize>(&self) -> Result<[Num; N]> {
        self.counts(&[N])?;
        self.nums(0)
    }
}

/// The statements of a tag script, as the engine's CScriptInterpreter splits them.
fn statements(text: &str) -> Result<Vec<Stmt>> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let (mut i, mut line) = (0, 1);
    let mut out = Vec::new();
    // skips white space and comments, counting lines
    let skip = |i: &mut usize, line: &mut usize| loop {
        if *i < n && chars[*i].is_whitespace() {
            if chars[*i] == '\n' {
                *line += 1;
            }
            *i += 1;
        } else if *i + 1 < n && chars[*i] == '/' && chars[*i + 1] == '/' {
            while *i < n && chars[*i] != '\n' {
                *i += 1;
            }
        } else if *i + 1 < n && chars[*i] == '/' && chars[*i + 1] == '*' {
            *i += 2;
            while *i < n && !(chars[*i] == '*' && *i + 1 < n && chars[*i + 1] == '/') {
                if chars[*i] == '\n' {
                    *line += 1;
                }
                *i += 1;
            }
            *i = (*i + 2).min(n);
        } else {
            break;
        }
    };
    loop {
        skip(&mut i, &mut line);
        if i >= n {
            break;
        }
        let start = i;
        while i < n && (chars[i].is_ascii_uppercase() || chars[i].is_ascii_digit() || chars[i] == '_') {
            i += 1;
        }
        if i == start || !chars[start].is_ascii_uppercase() {
            bail!("line {line}: expected a keyword at {:?}", chars[start]);
        }
        let keyword: String = chars[start..i].iter().collect();
        let at = line;
        let mut args = Vec::new();
        skip(&mut i, &mut line);
        if i < n && chars[i] == ';' {
            i += 1;
            out.push(Stmt { keyword, args, line: at });
            continue;
        }
        loop {
            skip(&mut i, &mut line);
            if i < n && chars[i] == '"' {
                let begin = i + 1;
                i = begin;
                while i < n && chars[i] != '"' && chars[i] != '\n' {
                    i += 1;
                }
                if i >= n || chars[i] != '"' {
                    bail!("line {line}: {keyword}: unterminated string");
                }
                args.push(Arg::Str(chars[begin..i].iter().collect()));
                i += 1;
            } else {
                let begin = i;
                while i < n && !matches!(chars[i], ',' | ';' | '\n') && !chars[i].is_whitespace() {
                    i += 1;
                }
                let token: String = chars[begin..i].iter().collect();
                match Num::parse(&token) {
                    Some(value) => args.push(Arg::Num(value)),
                    None => bail!("line {line}: {keyword}: {token:?} is not a number"),
                }
            }
            skip(&mut i, &mut line);
            match chars.get(i) {
                Some(',') => i += 1,
                Some(';') => {
                    i += 1;
                    break;
                }
                _ => bail!("line {line}: {keyword}: expected ',' or ';'"),
            }
        }
        out.push(Stmt { keyword, args, line: at });
    }
    Ok(out)
}

type Stmts = std::vec::IntoIter<Stmt>;

/// The next statement inside a block, or an error naming the block's end.
fn inside(stmts: &mut Stmts, end: &str) -> Result<Stmt> {
    stmts.next().ok_or_else(|| Error::invalid(format!("missing {end}")))
}

/// Sets a line's value once.
fn once<T>(slot: &mut Option<T>, stmt: &Stmt, value: T) -> Result<()> {
    if slot.is_some() {
        return Err(stmt.error("repeated"));
    }
    *slot = Some(value);
    Ok(())
}

fn required<T>(slot: Option<T>, keyword: &str, block: &str) -> Result<T> {
    slot.ok_or_else(|| Error::invalid(format!("{block} has no {keyword}")))
}

/// A block's end line, which has no arguments.
fn end(stmt: &Stmt) -> Result<()> {
    stmt.counts(&[0])
}

/// Reads a .map file.
pub fn parse(data: &[u8]) -> Result<MapFile> {
    let Some(text) = c2_disc::text::decode_strict(data) else {
        bail!("not cp932 text");
    };
    let mut stmts = statements(&text)?.into_iter();
    let mut map = MapFile::default();
    let mut order = Vec::new();
    let mut s = SettingsBuilder::default();
    while let Some(stmt) = stmts.next() {
        match stmt.keyword.as_str() {
            "IMG" => {
                stmt.counts(&[1])?;
                map.images.push(stmt.string(0)?);
                order.push(HeaderLine::Img);
            }
            "PCP" => {
                stmt.counts(&[1])?;
                map.packs.push(stmt.string(0)?);
                order.push(HeaderLine::Pcp);
            }
            "PARTS" => map.templates.push(template(&stmt, &mut stmts)?),
            "MAP_PARTS" => map.placements.push(placement(&stmt, &mut stmts)?),
            "CAMERA_INFO" => {
                let cameras = cameras(&stmt, &mut stmts)?;
                once(&mut map.cameras, &stmt, cameras)?;
            }
            "FUNC_POINT" => {
                let points = points(&stmt, &mut stmts)?;
                once(&mut map.points, &stmt, points)?;
            }
            "LIGHT_SET" => s.light_sets.push(light_set(&stmt, &mut stmts)?),
            "ACTIVE_LIGHT_SET" => once(&mut s.active_light_set, &stmt, stmt.one()?)?,
            "FLOOR" => once(&mut s.floor, &stmt, stmt.one()?)?,
            "CHARA_POS" => once(&mut s.chara_pos, &stmt, stmt.exactly()?)?,
            "DEF_FOOT" => once(&mut s.def_foot, &stmt, stmt.one()?)?,
            "TIME_FLAG" => {
                stmt.counts(&[2, 3, 4])?;
                let flag = TimeFlag {
                    follow_clock: stmt.num(0)?,
                    blend: stmt.num(1)?,
                    fixed_time: stmt.optional::<1>(2)?.map(|[value]| value),
                    fixed_time_enable: stmt.optional::<1>(3)?.map(|[value]| value),
                };
                once(&mut s.time_flag, &stmt, flag)?;
            }
            "TIME_LIGHT_NUM" => once(&mut s.time_light_num, &stmt, stmt.one()?)?,
            "TIME_CFADE" => once(&mut s.time_cfade, &stmt, stmt.one()?)?,
            "SKY_INFO" => {
                stmt.counts(&[2, 3])?;
                let sky = SkyInfo {
                    sky: stmt.num(0)?,
                    offset: stmt.num(1)?,
                    sun_angle: stmt.optional::<1>(2)?.map(|[value]| value),
                };
                once(&mut s.sky_info, &stmt, sky)?;
            }
            "LENS_FLARE" => once(&mut s.lens_flare, &stmt, stmt.one()?)?,
            "ALL_SCISSOR" => once(&mut s.all_scissor, &stmt, stmt.one()?)?,
            "CHARA_LIGHT_ADJUST" => once(&mut s.chara_light_adjust, &stmt, stmt.exactly()?)?,
            _ => return Err(stmt.error("not expected at the top level")),
        }
    }
    let images_first: Vec<HeaderLine> =
        map.images.iter().map(|_| HeaderLine::Img).chain(map.packs.iter().map(|_| HeaderLine::Pcp)).collect();
    if order != images_first {
        map.header_order = Some(order);
    }
    map.settings = s.build()?;
    Ok(map)
}

#[derive(Default)]
struct SettingsBuilder {
    active_light_set: Option<Num>,
    light_sets: Vec<LightSet>,
    floor: Option<Num>,
    chara_pos: Option<[Num; 3]>,
    def_foot: Option<Num>,
    time_flag: Option<TimeFlag>,
    time_light_num: Option<Num>,
    time_cfade: Option<Num>,
    sky_info: Option<SkyInfo>,
    lens_flare: Option<Num>,
    all_scissor: Option<Num>,
    chara_light_adjust: Option<[Num; 4]>,
}

impl SettingsBuilder {
    fn build(self) -> Result<Settings> {
        const MAP: &str = "the map";
        Ok(Settings {
            active_light_set: required(self.active_light_set, "ACTIVE_LIGHT_SET", MAP)?,
            light_sets: self.light_sets,
            floor: required(self.floor, "FLOOR", MAP)?,
            chara_pos: required(self.chara_pos, "CHARA_POS", MAP)?,
            def_foot: required(self.def_foot, "DEF_FOOT", MAP)?,
            time_flag: required(self.time_flag, "TIME_FLAG", MAP)?,
            time_light_num: required(self.time_light_num, "TIME_LIGHT_NUM", MAP)?,
            time_cfade: self.time_cfade,
            sky_info: required(self.sky_info, "SKY_INFO", MAP)?,
            lens_flare: self.lens_flare,
            all_scissor: self.all_scissor,
            chara_light_adjust: self.chara_light_adjust,
        })
    }
}

fn template(head: &Stmt, stmts: &mut Stmts) -> Result<Template> {
    head.counts(&[1])?;
    let name = head.string(0)?;
    let block = format!("PARTS {name:?}");
    let (mut far_clip, mut light_flag, mut move_flag, mut points_) = (None, None, None, None);
    let mut pieces = Vec::new();
    loop {
        let stmt = inside(stmts, "PARTS_END")?;
        match stmt.keyword.as_str() {
            "PARTS_END" => {
                end(&stmt)?;
                break;
            }
            "FAR_CLIP" => once(&mut far_clip, &stmt, stmt.exactly()?)?,
            "LIGHT_FLAG" => once(&mut light_flag, &stmt, stmt.exactly()?)?,
            "MOVE_FLAG" => once(&mut move_flag, &stmt, stmt.exactly()?)?,
            "PIECE" => pieces.push(piece(&stmt, stmts)?),
            "FUNC_POINT" => {
                let found = points(&stmt, stmts)?;
                once(&mut points_, &stmt, found)?;
            }
            _ => return Err(stmt.error(format!("not expected in {block}"))),
        }
    }
    Ok(Template {
        light_flag: required(light_flag, "LIGHT_FLAG", &block)?,
        name,
        far_clip,
        move_flag,
        pieces,
        points: points_,
    })
}

fn piece(head: &Stmt, stmts: &mut Stmts) -> Result<Piece> {
    head.counts(&[2])?;
    let (model, show) = (head.string(0)?, head.num(1)?);
    let block = format!("PIECE {model:?}");
    let (mut name, mut pos, mut rot, mut scale, mut materials, mut col_type, mut time) =
        (None, None, None, None, None, None, None);
    loop {
        let stmt = inside(stmts, "PIECE_END")?;
        match stmt.keyword.as_str() {
            "PIECE_END" => {
                end(&stmt)?;
                break;
            }
            "PIECE_NAME" => {
                stmt.counts(&[1])?;
                once(&mut name, &stmt, stmt.string(0)?)?;
            }
            "PIECE_POS" => once(&mut pos, &stmt, stmt.exactly()?)?,
            "PIECE_ROT" => once(&mut rot, &stmt, stmt.exactly()?)?,
            "PIECE_SCALE" => once(&mut scale, &stmt, stmt.exactly()?)?,
            "PIECE_MATERIAL_START" => {
                stmt.one()?;
                let mut found = Vec::new();
                loop {
                    let line = inside(stmts, "PIECE_MATERIAL_END")?;
                    match line.keyword.as_str() {
                        "PIECE_MATERIAL_END" => {
                            end(&line)?;
                            break;
                        }
                        "PIECE_MATERIAL" => {
                            line.counts(&[7])?;
                            found.push(PieceMaterial {
                                frame: line.string(0)?,
                                material: line.num(1)?,
                                color: line.nums(2)?,
                                flag: line.num(6)?,
                            });
                        }
                        _ => return Err(line.error("not expected in PIECE_MATERIAL_START")),
                    }
                }
                once(&mut materials, &stmt, found)?;
            }
            "PIECE_COL_TYPE" => {
                stmt.counts(&[1, 2])?;
                let value = ColType { kind: stmt.num(0)?, param: stmt.optional::<1>(1)?.map(|[value]| value) };
                once(&mut col_type, &stmt, value)?;
            }
            "PIECE_TIME" => once(&mut time, &stmt, stmt.exactly()?)?,
            _ => return Err(stmt.error(format!("not expected in {block}"))),
        }
    }
    Ok(Piece {
        name: required(name, "PIECE_NAME", &block)?,
        pos: required(pos, "PIECE_POS", &block)?,
        rot: required(rot, "PIECE_ROT", &block)?,
        scale: required(scale, "PIECE_SCALE", &block)?,
        time: required(time, "PIECE_TIME", &block)?,
        model,
        show,
        materials,
        col_type,
    })
}

fn placement(head: &Stmt, stmts: &mut Stmts) -> Result<Placement> {
    head.counts(&[2])?;
    let (name, show) = (head.string(0)?, head.num(1)?);
    let block = format!("MAP_PARTS {name:?}");
    let (mut far_clip, mut template, mut group, mut pos, mut rot, mut scale) = (None, None, None, None, None, None);
    loop {
        let stmt = inside(stmts, "MAP_PARTS_END")?;
        match stmt.keyword.as_str() {
            "MAP_PARTS_END" => {
                end(&stmt)?;
                break;
            }
            "MAP_FAR_CLIP" => once(&mut far_clip, &stmt, stmt.exactly()?)?,
            "PARTS_NAME" => {
                stmt.counts(&[1])?;
                once(&mut template, &stmt, stmt.string(0)?)?;
            }
            "PARTS_GROUP" => {
                stmt.counts(&[1])?;
                once(&mut group, &stmt, stmt.string(0)?)?;
            }
            "PARTS_POS" => once(&mut pos, &stmt, stmt.exactly()?)?,
            "PARTS_ROT" => once(&mut rot, &stmt, stmt.exactly()?)?,
            "PARTS_SCALE" => once(&mut scale, &stmt, stmt.exactly()?)?,
            _ => return Err(stmt.error(format!("not expected in {block}"))),
        }
    }
    Ok(Placement {
        template: required(template, "PARTS_NAME", &block)?,
        group: required(group, "PARTS_GROUP", &block)?,
        pos: required(pos, "PARTS_POS", &block)?,
        rot: required(rot, "PARTS_ROT", &block)?,
        scale: required(scale, "PARTS_SCALE", &block)?,
        name,
        show,
        far_clip,
    })
}

fn light_set(head: &Stmt, stmts: &mut Stmts) -> Result<LightSet> {
    let index = head.one()?;
    let block = format!("LIGHT_SET {}", index.text());
    let (mut fov, mut bgcolor, mut bgcolor2, mut ambient, mut fog_enable, mut fog) =
        (None, None, None, None, None, None);
    let mut lights = Vec::new();
    loop {
        let stmt = inside(stmts, "LIGHT_SET_END")?;
        match stmt.keyword.as_str() {
            "LIGHT_SET_END" => {
                end(&stmt)?;
                break;
            }
            "FOV" => once(&mut fov, &stmt, stmt.one()?)?,
            "BGCOLOR" => once(&mut bgcolor, &stmt, stmt.exactly()?)?,
            "BGCOLOR2" => once(&mut bgcolor2, &stmt, stmt.exactly()?)?,
            "AMBIENT" => once(&mut ambient, &stmt, stmt.exactly()?)?,
            "LIGHT" => {
                stmt.counts(&[4, 7])?;
                lights.push(SetLight::Light(Light {
                    index: stmt.num(0)?,
                    dir: stmt.nums(1)?,
                    color: stmt.optional(4)?,
                }));
            }
            "PLIGHT" => {
                stmt.counts(&[8])?;
                lights.push(SetLight::Plight(PointLight {
                    index: stmt.num(0)?,
                    power: stmt.num(1)?,
                    pos: stmt.nums(2)?,
                    color: stmt.nums(5)?,
                }));
            }
            "FOG_ENABLE" => once(&mut fog_enable, &stmt, stmt.one()?)?,
            "FOG" => {
                stmt.counts(&[2, 5, 7])?;
                let value = Fog {
                    near: stmt.num(0)?,
                    far: stmt.num(1)?,
                    color: stmt.optional(2)?,
                    offset_far: stmt.optional(5)?,
                };
                once(&mut fog, &stmt, value)?;
            }
            _ => return Err(stmt.error(format!("not expected in {block}"))),
        }
    }
    Ok(LightSet {
        fov: required(fov, "FOV", &block)?,
        bgcolor: required(bgcolor, "BGCOLOR", &block)?,
        bgcolor2: required(bgcolor2, "BGCOLOR2", &block)?,
        ambient: required(ambient, "AMBIENT", &block)?,
        fog_enable: required(fog_enable, "FOG_ENABLE", &block)?,
        fog: required(fog, "FOG", &block)?,
        index,
        lights,
    })
}

fn cameras(head: &Stmt, stmts: &mut Stmts) -> Result<Vec<FixCamera>> {
    head.one()?;
    let mut found = Vec::new();
    loop {
        let stmt = inside(stmts, "CAMERA_INFO_END")?;
        match stmt.keyword.as_str() {
            "CAMERA_INFO_END" => {
                end(&stmt)?;
                return Ok(found);
            }
            "FIX_CAMERA" => {
                end(&stmt)?;
                found.push(camera(stmts)?);
            }
            _ => return Err(stmt.error("not expected in CAMERA_INFO")),
        }
    }
}

fn camera(stmts: &mut Stmts) -> Result<FixCamera> {
    let mut pos = None;
    let mut camera = FixCamera::default();
    loop {
        let stmt = inside(stmts, "FIX_CAMERA_END")?;
        match stmt.keyword.as_str() {
            "FIX_CAMERA_END" => {
                end(&stmt)?;
                break;
            }
            "FIX_CAMERA_POS" => once(&mut pos, &stmt, stmt.exactly()?)?,
            "FIX_CAMERA_POS2" => {
                stmt.counts(&[4])?;
                camera.pos2.push(CameraPos { index: stmt.num(0)?, pos: stmt.nums(1)? });
            }
            "FIX_CAMERA_OFF_GROUP" => {
                stmt.counts(&[3])?;
                camera.off_groups.push(CameraOffGroup {
                    index: stmt.num(0)?,
                    group: stmt.string(1)?,
                    value: stmt.num(2)?,
                });
            }
            "FIX_CAMERA_RECT" => {
                stmt.counts(&[16])?;
                camera.rects.push(CameraRect {
                    name: stmt.string(0)?,
                    min: stmt.nums(1)?,
                    max: stmt.nums(4)?,
                    pos: stmt.nums(7)?,
                    rot: stmt.nums(10)?,
                    scale: stmt.nums(13)?,
                });
            }
            _ => return Err(stmt.error("not expected in FIX_CAMERA")),
        }
    }
    camera.pos = required(pos, "FIX_CAMERA_POS", "FIX_CAMERA")?;
    Ok(camera)
}

/// A FUNC_POINT block's points (its count is not kept: the engine ignores it and the writer
/// computes it).
fn points(head: &Stmt, stmts: &mut Stmts) -> Result<Vec<Point>> {
    head.one()?;
    let mut found = Vec::new();
    loop {
        let stmt = inside(stmts, "FUNC_POINT_END")?;
        match stmt.keyword.as_str() {
            "FUNC_POINT_END" => {
                end(&stmt)?;
                return Ok(found);
            }
            "FUNC_DATA" => found.push(point(&stmt, stmts)?),
            _ => return Err(stmt.error("not expected in FUNC_POINT")),
        }
    }
}

fn point(head: &Stmt, stmts: &mut Stmts) -> Result<Point> {
    head.counts(&[2])?;
    let kind_name = head.string(0)?;
    let Some(kind) = PointKind::from_name(&kind_name) else {
        return Err(head.error(format!("unknown kind {kind_name:?}")));
    };
    let enable = head.num(1)?;
    let (mut name, mut flag, mut data, mut pos) = (None, None, None, None);
    loop {
        let stmt = inside(stmts, "FUNC_DATA_END")?;
        let value = match stmt.keyword.as_str() {
            "FUNC_DATA_END" => {
                end(&stmt)?;
                break;
            }
            "FUNC_NAME" => {
                stmt.counts(&[1])?;
                once(&mut name, &stmt, stmt.string(0)?)?;
                continue;
            }
            "FUNC_FLAG" => {
                once(&mut flag, &stmt, stmt.exactly()?)?;
                continue;
            }
            "FUNC_POS" => {
                once(&mut pos, &stmt, stmt.exactly::<9>()?)?;
                continue;
            }
            "FUNC_INVENT_DATA" => {
                stmt.counts(&[10])?;
                PointData::Invent(InventData {
                    id: stmt.num(0)?,
                    min: stmt.nums(1)?,
                    max: stmt.nums(4)?,
                    int_param: stmt.num(7)?,
                    float_param: stmt.num(8)?,
                    angle: stmt.num(9)?,
                })
            }
            "FUNC_EVENT_DATA" => {
                stmt.counts(&[10])?;
                PointData::Event(EventData {
                    kind: stmt.string(0)?,
                    event_no: stmt.num(1)?,
                    program: stmt.num(2)?,
                    args: stmt.nums(3)?,
                    target: stmt.string(6)?,
                    action_button: stmt.num(7)?,
                    use_item: stmt.num(8)?,
                    flag_100: stmt.num(9)?,
                })
            }
            "FUNC_FIRE_DATA" => {
                stmt.counts(&[4, 6])?;
                PointData::Fire(FireData { color: stmt.nums(0)?, draw: stmt.num(3)?, extra: stmt.optional(4)? })
            }
            "FUNC_PLIGHT_DATA" => {
                stmt.counts(&[12])?;
                PointData::Plight(PlightData {
                    power: stmt.num(0)?,
                    color: stmt.nums(1)?,
                    mode: stmt.num(4)?,
                    ints: stmt.nums(5)?,
                    flicker_type: stmt.num(9)?,
                    flicker_depth: stmt.num(10)?,
                    flicker_period: stmt.num(11)?,
                })
            }
            "FUNC_EFFECT_NAME" => {
                stmt.counts(&[1])?;
                PointData::Effect(stmt.string(0)?)
            }
            "FUNC_SOUND_DATA" => {
                stmt.counts(&[11])?;
                PointData::Sound(SoundData {
                    number: stmt.num(0)?,
                    near: stmt.num(1)?,
                    far: stmt.num(2)?,
                    int_param: stmt.num(3)?,
                    shape: stmt.num(4)?,
                    start: stmt.nums(5)?,
                    end: stmt.nums(8)?,
                })
            }
            "FUNC_ANIME_DATA" => {
                stmt.counts(&[14, 16])?;
                PointData::Anime(AnimeData {
                    placement: stmt.string(0)?,
                    piece: stmt.string(1)?,
                    frame: stmt.string(2)?,
                    drives: stmt.num(3)?,
                    mode: stmt.num(4)?,
                    start: stmt.nums(5)?,
                    step: stmt.nums(8)?,
                    end: stmt.nums(11)?,
                    flags: stmt.optional(14)?,
                })
            }
            _ => return Err(stmt.error("not expected in FUNC_DATA")),
        };
        once(&mut data, &stmt, value)?;
    }
    let block = format!("FUNC_DATA {kind_name:?}");
    let place: [Num; 9] = required(pos, "FUNC_POS", &block)?;
    Ok(Point {
        name: required(name, "FUNC_NAME", &block)?,
        flag: required(flag, "FUNC_FLAG", &block)?,
        data: data.unwrap_or_default(),
        pos: [place[0], place[1], place[2]],
        rot: [place[3], place[4], place[5]],
        scale: [place[6], place[7], place[8]],
        kind,
        enable,
    })
}

// ---------- writing ----------

enum W<'a> {
    S(&'a str),
    N(Num),
}

fn nums<'a, const N: usize>(values: [Num; N]) -> impl Iterator<Item = W<'a>> {
    values.into_iter().map(W::N)
}

struct Writer {
    text: String,
}

/// The spaces the exporter put after some commas: (argument index, separator before it).
fn separators(keyword: &str) -> &'static [(usize, &'static str)] {
    match keyword {
        "FUNC_FLAG" => &[(1, ", "), (2, ", ")],
        "FUNC_POS" => &[(3, ", "), (6, ", ")],
        "FUNC_INVENT_DATA" => &[(1, ", ")],
        "FIX_CAMERA_RECT" => &[(1, ", "), (7, ", "), (10, ", "), (13, ", ")],
        _ => &[],
    }
}

impl Writer {
    fn line<'a>(&mut self, indent: &str, keyword: &str, args: impl IntoIterator<Item = W<'a>>) -> Result<()> {
        self.text.push_str(indent);
        self.text.push_str(keyword);
        let seps = separators(keyword);
        for (at, arg) in args.into_iter().enumerate() {
            self.text.push_str(match at {
                0 => " ",
                _ => seps.iter().find(|(index, _)| *index == at).map_or(",", |(_, sep)| *sep),
            });
            match arg {
                W::S(text) => {
                    if text.contains(['"', '\n', '\r']) {
                        bail!("{keyword}: a name cannot hold a quote or a line break: {text:?}");
                    }
                    self.text.push('"');
                    self.text.push_str(text);
                    self.text.push('"');
                }
                W::N(value) => self.text.push_str(&value.text()),
            }
        }
        self.text.push_str(";\n");
        Ok(())
    }

    fn blank(&mut self, lines: usize) {
        self.text.push_str(&"\n".repeat(lines));
    }

    fn template(&mut self, template: &Template) -> Result<()> {
        self.line("", "PARTS", [W::S(&template.name)])?;
        if let Some(far_clip) = template.far_clip {
            self.line("\t", "FAR_CLIP", nums(far_clip))?;
        }
        // the older exporter, which wrote no MOVE_FLAG, wrote LIGHT_FLAG without indentation
        let indent = if template.move_flag.is_some() { "\t" } else { "" };
        self.line(indent, "LIGHT_FLAG", nums(template.light_flag))?;
        if let Some(move_flag) = template.move_flag {
            self.line("\t", "MOVE_FLAG", nums(move_flag))?;
        }
        for piece in &template.pieces {
            self.piece(piece)?;
        }
        if let Some(points) = &template.points {
            self.points(points, 1)?;
        }
        self.line("", "PARTS_END", [])
    }

    fn piece(&mut self, piece: &Piece) -> Result<()> {
        self.line("\t", "PIECE", [W::S(&piece.model), W::N(piece.show)])?;
        self.line("\t\t", "PIECE_NAME", [W::S(&piece.name)])?;
        self.line("\t\t", "PIECE_POS", nums(piece.pos))?;
        self.line("\t\t", "PIECE_ROT", nums(piece.rot))?;
        self.line("\t\t", "PIECE_SCALE", nums(piece.scale))?;
        if let Some(materials) = &piece.materials {
            self.line("\t\t", "PIECE_MATERIAL_START", [W::N(count(materials.len()))])?;
            for material in materials {
                let args = [W::S(&material.frame), W::N(material.material)].into_iter();
                self.line("\t\t\t", "PIECE_MATERIAL", args.chain(nums(material.color)).chain([W::N(material.flag)]))?;
            }
            self.line("\t\t", "PIECE_MATERIAL_END", [])?;
        }
        if let Some(col_type) = &piece.col_type {
            self.line("\t\t", "PIECE_COL_TYPE", [W::N(col_type.kind)].into_iter().chain(col_type.param.map(W::N)))?;
        }
        self.line("\t\t", "PIECE_TIME", nums(piece.time))?;
        self.line("\t", "PIECE_END", [])
    }

    fn points(&mut self, points: &[Point], depth: usize) -> Result<()> {
        let tabs = |extra: usize| "\t".repeat(depth + extra);
        self.line(&tabs(0), "FUNC_POINT", [W::N(count(points.len()))])?;
        for point in points {
            self.line(&tabs(1), "FUNC_DATA", [W::S(point.kind.name()), W::N(point.enable)])?;
            let inner = tabs(2);
            self.line(&inner, "FUNC_NAME", [W::S(&point.name)])?;
            self.line(&inner, "FUNC_FLAG", nums(point.flag))?;
            match &point.data {
                PointData::None => {}
                PointData::Invent(data) => {
                    let args = [W::N(data.id)].into_iter().chain(nums(data.min)).chain(nums(data.max));
                    let args = args.chain(nums([data.int_param, data.float_param, data.angle]));
                    self.line(&inner, "FUNC_INVENT_DATA", args)?;
                }
                PointData::Event(data) => {
                    let args = [W::S(&data.kind)].into_iter().chain(nums([data.event_no, data.program]));
                    let args = args.chain(nums(data.args)).chain([W::S(&data.target)]);
                    let args = args.chain(nums([data.action_button, data.use_item, data.flag_100]));
                    self.line(&inner, "FUNC_EVENT_DATA", args)?;
                }
                PointData::Fire(data) => {
                    let args = nums(data.color).chain([W::N(data.draw)]);
                    self.line(&inner, "FUNC_FIRE_DATA", args.chain(data.extra.into_iter().flat_map(nums)))?;
                }
                PointData::Plight(data) => {
                    let args = [W::N(data.power)].into_iter().chain(nums(data.color)).chain([W::N(data.mode)]);
                    let args = args.chain(nums(data.ints));
                    let args = args.chain(nums([data.flicker_type, data.flicker_depth, data.flicker_period]));
                    self.line(&inner, "FUNC_PLIGHT_DATA", args)?;
                }
                PointData::Effect(name) => self.line(&inner, "FUNC_EFFECT_NAME", [W::S(name)])?,
                PointData::Sound(data) => {
                    let args = nums([data.number, data.near, data.far, data.int_param, data.shape]);
                    self.line(&inner, "FUNC_SOUND_DATA", args.chain(nums(data.start)).chain(nums(data.end)))?;
                }
                PointData::Anime(data) => {
                    let args = [W::S(&data.placement), W::S(&data.piece), W::S(&data.frame)].into_iter();
                    let args = args.chain(nums([data.drives, data.mode]));
                    let args = args.chain([data.start, data.step, data.end].into_iter().flat_map(nums));
                    self.line(&inner, "FUNC_ANIME_DATA", args.chain(data.flags.into_iter().flat_map(nums)))?;
                }
            }
            let place = nums(point.pos).chain(nums(point.rot)).chain(nums(point.scale));
            self.line(&inner, "FUNC_POS", place)?;
            self.line(&tabs(1), "FUNC_DATA_END", [])?;
        }
        self.line(&tabs(0), "FUNC_POINT_END", [])
    }

    fn settings(&mut self, settings: &Settings) -> Result<()> {
        self.line("", "ACTIVE_LIGHT_SET", [W::N(settings.active_light_set)])?;
        for set in &settings.light_sets {
            self.light_set(set)?;
        }
        self.line("", "FLOOR", [W::N(settings.floor)])?;
        self.line("", "CHARA_POS", nums(settings.chara_pos))?;
        self.line("", "DEF_FOOT", [W::N(settings.def_foot)])?;
        let time = &settings.time_flag;
        let args = [time.follow_clock, time.blend].into_iter().chain(time.fixed_time).chain(time.fixed_time_enable);
        self.line("", "TIME_FLAG", args.map(W::N))?;
        self.line("", "TIME_LIGHT_NUM", [W::N(settings.time_light_num)])?;
        if let Some(value) = settings.time_cfade {
            self.line("", "TIME_CFADE", [W::N(value)])?;
        }
        let sky = &settings.sky_info;
        self.line("", "SKY_INFO", [sky.sky, sky.offset].into_iter().chain(sky.sun_angle).map(W::N))?;
        if let Some(value) = settings.lens_flare {
            self.line("", "LENS_FLARE", [W::N(value)])?;
        }
        if let Some(value) = settings.all_scissor {
            self.line("", "ALL_SCISSOR", [W::N(value)])?;
        }
        if let Some(values) = settings.chara_light_adjust {
            self.line("", "CHARA_LIGHT_ADJUST", nums(values))?;
        }
        Ok(())
    }

    fn light_set(&mut self, set: &LightSet) -> Result<()> {
        // light set bodies are indented with one space
        self.line("", "LIGHT_SET", [W::N(set.index)])?;
        self.line(" ", "FOV", [W::N(set.fov)])?;
        self.line(" ", "BGCOLOR", nums(set.bgcolor))?;
        self.line(" ", "BGCOLOR2", nums(set.bgcolor2))?;
        self.line(" ", "AMBIENT", nums(set.ambient))?;
        for light in &set.lights {
            match light {
                SetLight::Light(light) => {
                    let args = [W::N(light.index)].into_iter().chain(nums(light.dir));
                    self.line(" ", "LIGHT", args.chain(light.color.into_iter().flat_map(nums)))?;
                }
                SetLight::Plight(light) => {
                    let args = nums([light.index, light.power]).chain(nums(light.pos)).chain(nums(light.color));
                    self.line(" ", "PLIGHT", args)?;
                }
            }
        }
        self.line(" ", "FOG_ENABLE", [W::N(set.fog_enable)])?;
        let fog = &set.fog;
        let args = nums([fog.near, fog.far]).chain(fog.color.into_iter().flat_map(nums));
        self.line(" ", "FOG", args.chain(fog.offset_far.into_iter().flat_map(nums)))?;
        self.line("", "LIGHT_SET_END", [])
    }

    fn placement(&mut self, placement: &Placement) -> Result<()> {
        self.line("", "MAP_PARTS", [W::S(&placement.name), W::N(placement.show)])?;
        if let Some(far_clip) = placement.far_clip {
            self.line("\t", "MAP_FAR_CLIP", nums(far_clip))?;
        }
        self.line("\t", "PARTS_NAME", [W::S(&placement.template)])?;
        self.line("\t", "PARTS_GROUP", [W::S(&placement.group)])?;
        self.line("\t", "PARTS_POS", nums(placement.pos))?;
        self.line("\t", "PARTS_ROT", nums(placement.rot))?;
        self.line("\t", "PARTS_SCALE", nums(placement.scale))?;
        self.line("", "MAP_PARTS_END", [])
    }

    fn cameras(&mut self, cameras: &[FixCamera]) -> Result<()> {
        self.line("", "CAMERA_INFO", [W::N(count(cameras.len()))])?;
        for camera in cameras {
            self.line("\t", "FIX_CAMERA", [])?;
            self.line("\t\t", "FIX_CAMERA_POS", nums(camera.pos))?;
            for pos in &camera.pos2 {
                self.line("\t\t", "FIX_CAMERA_POS2", [W::N(pos.index)].into_iter().chain(nums(pos.pos)))?;
            }
            for off in &camera.off_groups {
                self.line("\t\t", "FIX_CAMERA_OFF_GROUP", [W::N(off.index), W::S(&off.group), W::N(off.value)])?;
            }
            for rect in &camera.rects {
                let args = [W::S(&rect.name)].into_iter().chain(nums(rect.min)).chain(nums(rect.max));
                let args = args.chain(nums(rect.pos)).chain(nums(rect.rot)).chain(nums(rect.scale));
                self.line("\t\t", "FIX_CAMERA_RECT", args)?;
            }
            self.line("\t", "FIX_CAMERA_END", [])?;
        }
        self.line("", "CAMERA_INFO_END", [])
    }
}

fn count(n: usize) -> Num {
    Num::Int(i32::try_from(n).unwrap_or(i32::MAX))
}

/// Writes a .map file in the layout of the game's exporter (docs/assets/maps.md 2.3). Fails on a
/// name cp932 cannot write or one holding a quote or a line break.
pub fn write(map: &MapFile) -> Result<Vec<u8>> {
    let mut out = Writer { text: String::new() };
    let (mut images, mut packs) = (map.images.iter(), map.packs.iter());
    for line in map.header_order.iter().flatten() {
        match line {
            HeaderLine::Img => {
                if let Some(name) = images.next() {
                    out.line("", "IMG", [W::S(name)])?;
                }
            }
            HeaderLine::Pcp => {
                if let Some(name) = packs.next() {
                    out.line("", "PCP", [W::S(name)])?;
                }
            }
        }
    }
    for name in images {
        out.line("", "IMG", [W::S(name)])?;
    }
    for name in packs {
        out.line("", "PCP", [W::S(name)])?;
    }
    for template in &map.templates {
        out.template(template)?;
    }
    out.blank(2);
    out.settings(&map.settings)?;
    out.blank(1);
    for placement in &map.placements {
        out.placement(placement)?;
    }
    out.blank(2);
    if let Some(cameras) = &map.cameras {
        out.cameras(cameras)?;
    }
    out.blank(2);
    if let Some(points) = &map.points {
        out.points(points, 0)?;
    }
    out.blank(3);
    match c2_disc::text::encode(&out.text) {
        Some(bytes) => Ok(bytes),
        None => bail!("a name has a character cp932 cannot write"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small map in the exporter's layout: two IMG lines, a template with a material override,
    /// both col type forms and a point, an old exporter's template (no MOVE_FLAG), two light sets
    /// (one with a gap), a placement with a far clip, a fixed camera and the map's own points.
    const SAMPLE: &str = concat!(
        "IMG \"s_99.img\";\n",
        "IMG \"s_99b.img\";\n",
        "PCP \"s99_0.pcp\";\n",
        "PARTS \"p01_s99\";\n",
        "\tFAR_CLIP 2000,0;\n",
        "\tLIGHT_FLAG 0,0;\n",
        "\tMOVE_FLAG 1,1,1,1;\n",
        "\tPIECE \"s99_01-m.mds\",1;\n",
        "\t\tPIECE_NAME \"s99_01-m0_1\";\n",
        "\t\tPIECE_POS 0,-12.5,0;\n",
        "\t\tPIECE_ROT 0,2.35619,0;\n",
        "\t\tPIECE_SCALE -1,1,1;\n",
        "\t\tPIECE_MATERIAL_START 2;\n",
        "\t\t\tPIECE_MATERIAL \"grid54--f2appz0n1t-5_1\",0,1,0.5,0.5,1,0;\n",
        "\t\t\tPIECE_MATERIAL \"門\",1,1,1,1,0.5,1;\n",
        "\t\tPIECE_MATERIAL_END;\n",
        "\t\tPIECE_COL_TYPE 0,0;\n",
        "\t\tPIECE_TIME 20,5;\n",
        "\tPIECE_END;\n",
        "\tPIECE \"s99_01-a.mds\",0;\n",
        "\t\tPIECE_NAME \"s99_01-a0_1\";\n",
        "\t\tPIECE_POS 0,0,0;\n",
        "\t\tPIECE_ROT 8.4208e-007,0,-1.2e-005;\n",
        "\t\tPIECE_SCALE 1,1,1;\n",
        "\t\tPIECE_COL_TYPE 1;\n",
        "\t\tPIECE_TIME 0,0;\n",
        "\tPIECE_END;\n",
        "\tFUNC_POINT 2;\n",
        "\t\tFUNC_DATA \"invent\",1;\n",
        "\t\t\tFUNC_NAME \"invent_30001-0\";\n",
        "\t\t\tFUNC_FLAG 0, 0, 0,0;\n",
        "\t\t\tFUNC_INVENT_DATA 30001, -5.41372,-15.3,-4.44097,5.41372,15.3,4.44097,0,0,0;\n",
        "\t\t\tFUNC_POS 66.7974,56.8445,1179.71, 0,0,0, 1,1,1;\n",
        "\t\tFUNC_DATA_END;\n",
        "\t\tFUNC_DATA \"anime\",1;\n",
        "\t\t\tFUNC_NAME \"anime0\";\n",
        "\t\t\tFUNC_FLAG 0, 0, 0,0;\n",
        "\t\t\tFUNC_ANIME_DATA \"p01_s99\",\"s99_01-m0_1\",\"fog00\",2,1,0,0,0,0,0.02,0,0,180,0;\n",
        "\t\t\tFUNC_POS 0,0,0, 0,0,0, 1,1,1;\n",
        "\t\tFUNC_DATA_END;\n",
        "\tFUNC_POINT_END;\n",
        "PARTS_END;\n",
        "PARTS \"door84\";\n",
        "LIGHT_FLAG 0,0;\n",
        "PARTS_END;\n",
        "\n\n",
        "ACTIVE_LIGHT_SET 0;\n",
        "LIGHT_SET 0;\n",
        " FOV 52;\n",
        " BGCOLOR 60,140,230;\n",
        " BGCOLOR2 0,0,0;\n",
        " AMBIENT 65,65,60;\n",
        " PLIGHT 0,300,10,20,30,255,200,100;\n",
        " LIGHT 0,1,1,1,200,190,185;\n",
        " FOG_ENABLE 1;\n",
        " FOG 500,3000,220,200,180,175,255;\n",
        "LIGHT_SET_END;\n",
        "LIGHT_SET 8;\n",
        " FOV 45;\n",
        " BGCOLOR 0,0,0;\n",
        " BGCOLOR2 0,0,0;\n",
        " AMBIENT 128,128,128;\n",
        " FOG_ENABLE 0;\n",
        " FOG 500,3000,0,0,0;\n",
        "LIGHT_SET_END;\n",
        "FLOOR -10000;\n",
        "CHARA_POS 0,0,0;\n",
        "DEF_FOOT 2;\n",
        "TIME_FLAG 1,1,12,0;\n",
        "TIME_LIGHT_NUM 4;\n",
        "TIME_CFADE 0;\n",
        "SKY_INFO 1,-900,230;\n",
        "LENS_FLARE 1;\n",
        "ALL_SCISSOR 0;\n",
        "CHARA_LIGHT_ADJUST 0,0.3,1.8,0.44;\n",
        "\n",
        "MAP_PARTS \"p01_s99\",1;\n",
        "\tMAP_FAR_CLIP 2500,1;\n",
        "\tPARTS_NAME \"p01_s99\";\n",
        "\tPARTS_GROUP \"\";\n",
        "\tPARTS_POS 0,0,0;\n",
        "\tPARTS_ROT 0,1.5708,0;\n",
        "\tPARTS_SCALE 1,1,1;\n",
        "MAP_PARTS_END;\n",
        "\n\n",
        "CAMERA_INFO 1;\n",
        "\tFIX_CAMERA;\n",
        "\t\tFIX_CAMERA_POS 10,50,0;\n",
        "\t\tFIX_CAMERA_POS2 0,10,50,0;\n",
        "\t\tFIX_CAMERA_OFF_GROUP 0,\"\",0;\n",
        "\t\tFIX_CAMERA_RECT \"box\", -48.6805,-100,-100.91,125.092,136.277,95.3663, 25.7793,57.0068,54.2182, 0,0,0, 1,1,1;\n",
        "\tFIX_CAMERA_END;\n",
        "CAMERA_INFO_END;\n",
        "\n\n",
        "FUNC_POINT 3;\n",
        "\tFUNC_DATA \"event\",1;\n",
        "\t\tFUNC_NAME \"talk\";\n",
        "\t\tFUNC_FLAG 0, 0, 20,5;\n",
        "\t\tFUNC_EVENT_DATA \"event\",0,500,0,0,0,\"\",1,0,0;\n",
        "\t\tFUNC_POS 68.04,41.85,1180.61, 0,0,0, 30,30,30;\n",
        "\tFUNC_DATA_END;\n",
        "\tFUNC_DATA \"flare\",0;\n",
        "\t\tFUNC_NAME \"flare0\";\n",
        "\t\tFUNC_FLAG 0, 0, 0,0;\n",
        "\t\tFUNC_FIRE_DATA 0,0,0,1;\n",
        "\t\tFUNC_POS 1,2,3, 0,0,0, 1,1,1;\n",
        "\tFUNC_DATA_END;\n",
        "\tFUNC_DATA \"pos\",1;\n",
        "\t\tFUNC_NAME \"point\";\n",
        "\t\tFUNC_FLAG 0, 0, 0,0;\n",
        "\t\tFUNC_POS -259.7,0,792.4, 0,-1.5708,0, 1,1,1;\n",
        "\tFUNC_DATA_END;\n",
        "FUNC_POINT_END;\n",
        "\n\n\n",
    );

    fn sample() -> Vec<u8> {
        c2_disc::text::encode(SAMPLE).unwrap()
    }

    #[test]
    fn a_map_reads_into_its_tree_and_writes_back_byte_for_byte() {
        let map = parse(&sample()).unwrap();
        assert_eq!(write(&map).unwrap(), sample());
        assert_eq!(map.images, ["s_99.img", "s_99b.img"]);
        assert_eq!(map.header_order, None);
        let template = &map.templates[0];
        assert_eq!(template.far_clip, Some([Num::Int(2000), Num::Int(0)]));
        assert_eq!(template.pieces[0].rot[1], Num::Float(2.35619));
        assert_eq!(template.pieces[0].materials.as_ref().unwrap()[1].frame, "門");
        assert_eq!(template.pieces[1].col_type, Some(ColType { kind: Num::Int(1), param: None }));
        assert_eq!(template.pieces[1].rot[0], Num::Exponent { exponent: 8.4208e-7 });
        assert_eq!(template.pieces[1].rot[0].float(), 0.0);
        assert_eq!(map.templates[1].move_flag, None);
        let sets = &map.settings.light_sets;
        assert_eq!(
            (sets[1].index, sets[1].fog.color, sets[1].fog.offset_far),
            (Num::Int(8), Some([Num::Int(0); 3]), None)
        );
        assert!(matches!(sets[0].lights[0], SetLight::Plight(_)));
        let points = map.points.as_ref().unwrap();
        assert_eq!(points[0].kind, PointKind::Event);
        let PointData::Event(event) = &points[0].data else { panic!() };
        assert_eq!(event.program, Num::Int(500));
        assert_eq!(points[2].data, PointData::None);
        let PointData::Anime(anime) = &template.points.as_ref().unwrap()[1].data else { panic!() };
        assert_eq!(
            (anime.drives, anime.mode, anime.step[1], anime.flags),
            (Num::Int(2), Num::Int(1), Num::Float(0.02), None)
        );
        assert_eq!(map.cameras.as_ref().unwrap()[0].rects[0].max[2], Num::Float(95.3663));
    }

    #[test]
    fn the_tree_goes_through_json_unchanged() {
        let map = parse(&sample()).unwrap();
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains(r#"{"exponent":8.4208e-7}"#), "{json}");
        let back: MapFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);
        assert_eq!(write(&back).unwrap(), sample());
    }

    #[test]
    fn image_and_pack_lines_keep_an_unusual_order() {
        let text =
            SAMPLE.replacen("IMG \"s_99b.img\";\nPCP \"s99_0.pcp\";\n", "PCP \"s99_0.pcp\";\nIMG \"s_99b.img\";\n", 1);
        let bytes = c2_disc::text::encode(&text).unwrap();
        let map = parse(&bytes).unwrap();
        assert_eq!(map.header_order, Some(vec![HeaderLine::Img, HeaderLine::Pcp, HeaderLine::Img]));
        assert_eq!(write(&map).unwrap(), bytes);
    }

    #[test]
    fn numbers_are_written_as_the_exporter_wrote_them() {
        let text = |value: f32| Num::from(value).text();
        assert_eq!(text(1.0), "1");
        assert_eq!(Num::from(2.0), Num::Int(2));
        assert_eq!(text(-0.0), "0");
        assert_eq!(text(0.1), "0.1");
        assert_eq!(text(66.79743), "66.7974");
        assert_eq!(text(-1234.5678), "-1234.57");
        assert_eq!(text(0.0001), "0.0001");
        // %g would write these with exponents, which the engine reads as 0
        assert_eq!(text(1.234567e-5), "0.0000123457");
        assert_eq!(text(-8.4208e-7), "-0.00000084208");
        assert_eq!(text(1234567.0), "1234570");
        assert_eq!(Num::from(1.234567e-5), Num::Float(0.0000123457));
        assert_eq!(Num::Exponent { exponent: 8.4208e-7 }.text(), "8.4208e-007");
        assert_eq!(Num::Exponent { exponent: -1.2e-5 }.text(), "-1.2e-005");
        assert_eq!(Num::parse("-0"), Some(Num::Int(0)));
        assert_eq!(Num::parse("1.5e-005"), Some(Num::Exponent { exponent: 1.5e-5 }));
        assert_eq!(Num::parse("1x"), None);
        assert_eq!(Num::Float(2.9).int(), 2);
    }

    fn point(name: &str, at: [f32; 3]) -> Point {
        Point {
            kind: PointKind::Event,
            enable: 1.into(),
            name: name.into(),
            flag: [0.into(); 4],
            data: PointData::Event(EventData {
                kind: "event".into(),
                program: 9000.into(),
                action_button: 1.into(),
                ..EventData::default()
            }),
            pos: at.map(Num::from),
            rot: [0.into(); 3],
            scale: [30.into(); 3],
        }
    }

    #[test]
    fn edits_write_text_that_reads_back_to_the_same_tree() {
        // a map with no cameras and no points of its own
        let text = SAMPLE.split("CAMERA_INFO 1;").next().unwrap().to_string() + "\n\n\n\n\n";
        let mut map = parse(&c2_disc::text::encode(&text).unwrap()).unwrap();
        assert_eq!((map.cameras.as_ref(), map.points.as_ref()), (None, None));
        assert_eq!(write(&map).unwrap(), c2_disc::text::encode(&text).unwrap());

        // move a placement, with a value too small for %g to write without an exponent
        map.placements[0].pos = [12.5, 0.000001234, -300.25].map(Num::from);
        // add a piece to a template
        map.templates[1].pieces.push(Piece {
            model: "door84-m.mds".into(),
            show: 1.into(),
            name: "door84-m0_1".into(),
            pos: [0.into(); 3],
            rot: [0.0, 1.5707964, 0.0].map(Num::from),
            scale: [1.into(); 3],
            col_type: Some(ColType { kind: 0.into(), param: Some(0.into()) }),
            ..Piece::default()
        });
        // add a placement of it
        map.placements.push(Placement {
            name: "door84-1".into(),
            show: 1.into(),
            template: "door84".into(),
            pos: [100.0, 0.0, -0.00000003].map(Num::from),
            rot: [0.into(); 3],
            scale: [1.into(); 3],
            ..Placement::default()
        });
        // and a point to the map, which had none
        map.points = Some(vec![point("talk", [68.04, 41.85, 1180.61])]);

        let bytes = write(&map).unwrap();
        let written = c2_disc::text::decode_strict(&bytes).unwrap();
        assert!(written.contains("\tPARTS_POS 12.5,0.000001234,-300.25;\n"), "{written}");
        assert!(written.contains("\tPARTS_POS 100,0,-0.00000003;\n"), "{written}");
        assert!(written.contains("\tPIECE \"door84-m.mds\",1;\n\t\tPIECE_NAME \"door84-m0_1\";\n"));
        assert!(written.contains("\t\tPIECE_ROT 0,1.5708,0;\n"));
        assert!(written.ends_with(concat!(
            "MAP_PARTS_END;\n\n\n\n\n",
            "FUNC_POINT 1;\n",
            "\tFUNC_DATA \"event\",1;\n",
            "\t\tFUNC_NAME \"talk\";\n",
            "\t\tFUNC_FLAG 0, 0, 0,0;\n",
            "\t\tFUNC_EVENT_DATA \"event\",0,9000,0,0,0,\"\",1,0,0;\n",
            "\t\tFUNC_POS 68.04,41.85,1180.61, 0,0,0, 30,30,30;\n",
            "\tFUNC_DATA_END;\n",
            "FUNC_POINT_END;\n\n\n\n",
        )));
        // only the exponents the file already had are written as exponents
        let exponents = written.lines().filter(|line| line.contains("e-0")).count();
        assert_eq!(exponents, 1);
        assert_eq!(parse(&bytes).unwrap(), map);
    }

    #[test]
    fn broken_files_and_names_are_errors() {
        let text = SAMPLE.replacen("\tPIECE_END;\n", "", 1);
        assert!(parse(&c2_disc::text::encode(&text).unwrap()).is_err());
        let text = SAMPLE.replacen("FLOOR -10000;\n", "", 1);
        assert!(parse(&c2_disc::text::encode(&text).unwrap()).unwrap_err().to_string().contains("FLOOR"));
        let mut map = parse(&sample()).unwrap();
        map.templates[0].name = "a\"b".into();
        assert!(write(&map).is_err());
    }

    /// The whole disc: set C2S_CORPUS to an extracted disc (`extracted/disc`) to round-trip every
    /// .map under its `map` folder, through JSON too.
    #[test]
    fn corpus_maps_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        fn walk(folder: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("map")) {
                    found.push(path);
                }
            }
        }
        let mut paths = Vec::new();
        walk(&std::path::Path::new(&corpus).join("map"), &mut paths);
        assert!(!paths.is_empty(), "no .map files under {corpus:?}");
        let mut failed = Vec::new();
        for path in &paths {
            let data = std::fs::read(path).unwrap();
            let outcome = parse(&data).and_then(|map| {
                let json = serde_json::to_string(&map).map_err(|error| Error::invalid(error.to_string()))?;
                let back: MapFile = serde_json::from_str(&json).map_err(|error| Error::invalid(error.to_string()))?;
                if back != map {
                    bail!("changed through JSON");
                }
                write(&map)
            });
            match outcome {
                Ok(written) if written == data => {}
                Ok(_) => failed.push(format!("{}: written differently", path.display())),
                Err(error) => failed.push(format!("{}: {error}", path.display())),
            }
        }
        eprintln!("{} of {} .map files round-trip", paths.len() - failed.len(), paths.len());
        assert!(failed.is_empty(), "{}", failed.join("\n"));
    }
}
