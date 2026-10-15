//! The shops (docs/assets/data.md, "Shops"): prices and what each shop sells, by chapter, as one
//! JSON table, and edits of it written back into the shop lists.
//!
//! CShopMenu::InitEnd (menushop.cpp) reads `menu/shopusa<chapter>.cfg` (`menu/shoplst<chapter>.cfg`
//! when the language is Japanese), the chapter clamped to 1..8, with the tags `PRICE item, buy,
//! sell;` (CShop::price, by item number) and `SHOP id, item, ...;` (the goods of the shop the
//! event script opened, by its id; at most 63, the id one of a statement's 64 arguments). A later
//! statement for one item or shop fills it again, so it wins.
//!
//! The JSON gives each price and shop list once when every chapter has the same, and as a list of
//! eight (chapters 1 to 8) when they differ; a mod may give either for any record. An edit goes
//! into both sets of files, so every language sees it. Like the item table ([`crate::item_data`])
//! a mod's table holds only what it changes, and several mods' tables merge.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::cfg_text::{self, Arg, Edit, Script, To};
use crate::{bail, Result};

pub const FOLDER: &str = "menu";
pub const CHAPTERS: usize = 8;
/// The goods one shop can list: SHOP_ITEM_MAX is 64, but a statement holds at most
/// cfg_text::ARGS_MOST (64) arguments and the first is the shop id.
pub const SHOP_ITEM_MAX: usize = 63;
/// CShop::price has a slot for each item number below this; PRICE does not check.
pub const PRICE_SLOTS: i64 = 0x200;

/// The shop list files, by [`FOLDER`]: the eight chapters of the other languages', then of the
/// Japanese.
pub fn files() -> Vec<String> {
    let mut out: Vec<String> = (1..=CHAPTERS).map(|chapter| format!("shopusa{chapter}.cfg")).collect();
    out.extend((1..=CHAPTERS).map(|chapter| format!("shoplst{chapter}.cfg")));
    out
}

/// What a shop takes in payment (NowSellMode, by shop id in _SHOP_ANALYZE).
pub fn currency(shop: i32) -> &'static str {
    match shop {
        0x17 | 0x1C => "ridepod energy (ABS)",
        0x20 => "medals",
        0x21 => "free (Donny's shop, its goods from the invention level)",
        _ => "money",
    }
}

#[derive(Clone, Debug)]
pub struct Shops {
    scripts: Vec<Script>,
    /// {item: [statement index per file]}, None where a file has no PRICE for it.
    prices: BTreeMap<i32, Vec<Option<usize>>>,
    shops: BTreeMap<i32, Vec<Option<usize>>>,
    names: BTreeMap<i32, String>,
}

fn keyed(scripts: &[Script], tag: &str) -> BTreeMap<i32, Vec<Option<usize>>> {
    let mut out: BTreeMap<i32, Vec<Option<usize>>> = BTreeMap::new();
    for (file, script) in scripts.iter().enumerate() {
        for (at, statement) in script.statements.iter().enumerate().filter(|(_, statement)| statement.tag == tag) {
            if let Some(first) = statement.values().first() {
                out.entry(first.int()).or_insert_with(|| vec![None; scripts.len()])[file] = Some(at);
            }
        }
    }
    out
}

/// One value per chapter as the JSON gives it: a single value when all agree.
fn by_chapter(values: Vec<Value>) -> Value {
    if values.windows(2).all(|pair| pair[0] == pair[1]) {
        values.into_iter().next().unwrap_or(Value::Null)
    } else {
        Value::Array(values)
    }
}

impl Shops {
    /// The shops from the sixteen files (by [`files`]) and the English item names
    /// (comdatmes1.cfg) to label them.
    pub fn read(files: &[Vec<u8>], names: Option<&[u8]>) -> Result<Shops> {
        let wanted = self::files();
        if files.len() != wanted.len() {
            bail!("the shop lists are {} files", wanted.len());
        }
        let mut scripts = Vec::new();
        for (name, data) in wanted.iter().zip(files) {
            scripts.push(Script::parse(data).map_err(|error| crate::Error::invalid(format!("{name}: {error}")))?);
        }
        let mut shown = BTreeMap::new();
        if let Some(names) = names {
            for statement in Script::parse(names)?.statements.iter().filter(|statement| statement.tag == "MES_SYS") {
                if let [number, Arg::Str(text)] = &statement.values()[..] {
                    shown.insert(number.int(), dc2tk_disc::text::decode_lossy(text));
                }
            }
        }
        let prices = keyed(&scripts, "PRICE");
        let shops = keyed(&scripts, "SHOP");
        Ok(Shops { scripts, prices, shops, names: shown })
    }

    fn values(&self, file: usize, statement: usize) -> Vec<Arg> {
        self.scripts[file].statements[statement].values()
    }

    /// A price's buy (1) or sell (2) argument in each chapter of the files the export shows.
    fn price(&self, item: i32, arg: usize) -> Vec<Value> {
        let places = &self.prices[&item];
        (0..CHAPTERS)
            .map(|chapter| match places[chapter] {
                Some(at) => self.values(chapter, at).get(arg).map_or(Value::Null, |value| Value::from(value.int())),
                None => Value::Null,
            })
            .collect()
    }

    fn goods(&self, shop: i32, chapter: usize) -> Value {
        match self.shops[&shop][chapter] {
            Some(at) => {
                Value::Array(self.values(chapter, at)[1..].iter().map(|item| Value::from(item.int())).collect())
            }
            None => Value::Null,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut prices = Vec::new();
        for &item in self.prices.keys() {
            let mut object = Map::new();
            object.insert("item".into(), Value::from(item));
            if let Some(name) = self.names.get(&item) {
                object.insert("name".into(), Value::from(name.clone()));
            }
            object.insert("buy".into(), by_chapter(self.price(item, 1)));
            object.insert("sell".into(), by_chapter(self.price(item, 2)));
            prices.push(Value::Object(object));
        }
        let mut shops = Vec::new();
        for &shop in self.shops.keys() {
            let mut object = Map::new();
            object.insert("shop".into(), Value::from(shop));
            object.insert("currency".into(), Value::from(currency(shop)));
            let goods: Vec<Value> = (0..CHAPTERS).map(|chapter| self.goods(shop, chapter)).collect();
            let named = |list: &Value| -> Value {
                let names: Vec<Value> = list
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                let number = item.as_i64().unwrap_or(0) as i32;
                                Value::from(self.names.get(&number).cloned().unwrap_or_default())
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Value::Array(names)
            };
            match by_chapter(goods) {
                Value::Array(lists) if lists.iter().any(|list| list.is_array() || list.is_null()) => {
                    object.insert("names".into(), Value::Array(lists.iter().map(named).collect()));
                    object.insert("items".into(), Value::Array(lists));
                }
                list => {
                    if !self.names.is_empty() {
                        object.insert("names".into(), named(&list));
                    }
                    object.insert("items".into(), list);
                }
            }
            shops.push(Value::Object(object));
        }
        let mut top = Map::new();
        top.insert("prices".into(), Value::Array(prices));
        top.insert("shops".into(), Value::Array(shops));
        Value::Object(top)
    }

    /// Each chapter's value from a mod's field: one value for every chapter, or a list of eight.
    fn chapters<'a>(value: &'a Value, what: &str, list_valued: bool) -> Result<Vec<&'a Value>> {
        let per_chapter = match value.as_array() {
            Some(items) if !list_valued => Some(items),
            Some(items) if items.iter().all(Value::is_array) && items.len() == CHAPTERS => Some(items),
            _ => None,
        };
        match per_chapter {
            Some(items) if items.len() == CHAPTERS => Ok(items.iter().collect()),
            Some(_) => bail!("{what} is one value for every chapter or a list of {CHAPTERS}, chapters 1 to {CHAPTERS}"),
            None => Ok(vec![value; CHAPTERS]),
        }
    }

    /// The edits a mod's table makes: only values that differ from these shops'. Prices and shops
    /// are named by `item` and `shop`; `name`, `names` and `currency` are labels.
    pub fn changes(&self, table: &Value, module: &str) -> Result<Vec<Edit>> {
        self.changes_all(&[(module.to_string(), table.clone())])
    }

    /// The edits every mod's table makes, in order. A price for an item the lists lack is a new
    /// PRICE line in every file (its `buy` and `sell` both given), and a shop they lack a new SHOP
    /// line; a shop may list an item any of the mods prices.
    pub fn changes_all(&self, tables: &[(String, Value)]) -> Result<Vec<Edit>> {
        let mut priced = std::collections::BTreeSet::new();
        for (_, table) in tables {
            for record in table.get("prices").and_then(Value::as_array).into_iter().flatten() {
                if let Some(item) = record.get("item").and_then(Value::as_i64) {
                    if !self.prices.contains_key(&(item as i32)) {
                        priced.insert(item as i32);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for (module, table) in tables {
            out.extend(
                self.changes_with(table, module, &priced)
                    .map_err(|error| crate::Error::invalid(format!("mod {module}: {error}")))?,
            );
        }
        Ok(out)
    }

    /// The text of a new PRICE or SHOP line, added after the file's last line of that tag.
    fn added(&self, file: usize, tag: &str, args: Vec<Vec<u8>>, what: String, module: &str) -> Result<Edit> {
        let Some(after) = self.scripts[file].statements.iter().rposition(|statement| statement.tag == tag) else {
            bail!("{what}: {} has no {tag} line to add one after", files()[file]);
        };
        Ok(Edit {
            file,
            statement: after,
            arg: 0,
            to: To::Insert(cfg_text::statement_text(tag, &args)),
            what,
            module: module.to_string(),
        })
    }

    fn changes_with(&self, table: &Value, module: &str, priced: &std::collections::BTreeSet<i32>) -> Result<Vec<Edit>> {
        let Some(object) = table.as_object() else {
            bail!("a shop table is {{\"prices\": [...], \"shops\": [...]}}, as mod asset writes it");
        };
        for key in object.keys() {
            if key != "prices" && key != "shops" {
                bail!("a shop table has \"prices\" and \"shops\", not \"{key}\"");
            }
        }
        let list = |key: &str| -> Result<Vec<&Map<String, Value>>> {
            match object.get(key) {
                None => Ok(Vec::new()),
                Some(Value::Array(records)) => records
                    .iter()
                    .map(|record| {
                        record
                            .as_object()
                            .ok_or_else(|| crate::Error::invalid(format!("each of \"{key}\" is an object")))
                    })
                    .collect(),
                Some(_) => bail!("\"{key}\" is a list"),
            }
        };
        let mut out = Vec::new();
        let files = self.scripts.len();
        for record in list("prices")? {
            let Some(item) = record.get("item").and_then(Value::as_i64) else {
                bail!("each price names its item: {{\"item\": 11, \"buy\": 450}}");
            };
            let Some(places) = self.prices.get(&(item as i32)) else {
                // a new price: a PRICE line in every file
                if !(1..PRICE_SLOTS).contains(&item) {
                    bail!("price of item {item}: CShop::price has items 1 to {}", PRICE_SLOTS - 1);
                }
                let what = format!("new price of item {item}");
                for key in record.keys() {
                    if !["item", "name", "buy", "sell"].contains(&key.as_str()) {
                        bail!("{what}: \"{key}\" is not a field (buy, sell)");
                    }
                }
                let (Some(buy), Some(sell)) = (record.get("buy"), record.get("sell")) else {
                    bail!("{what}: a new price gives \"buy\" and \"sell\"");
                };
                let buys = Self::chapters(buy, &format!("{what} buy"), false)?;
                let sells = Self::chapters(sell, &format!("{what} sell"), false)?;
                for chapter in 0..CHAPTERS {
                    let mut args = vec![cfg_text::int_text(item)];
                    for (key, value) in [("buy", buys[chapter]), ("sell", sells[chapter])] {
                        let Some(number) = value.as_i64().filter(|number| (0..=i32::MAX as i64).contains(number))
                        else {
                            bail!("{what} {key} is a whole number from 0");
                        };
                        args.push(cfg_text::int_text(number));
                    }
                    for file in [chapter, chapter + CHAPTERS].into_iter().filter(|&file| file < files) {
                        out.push(self.added(file, "PRICE", args.clone(), what.clone(), module)?);
                    }
                }
                continue;
            };
            for (key, value) in record {
                let arg = match key.as_str() {
                    "item" | "name" => continue,
                    "buy" => 1,
                    "sell" => 2,
                    _ => bail!("price of item {item}: \"{key}\" is not a field (buy, sell)"),
                };
                let what = format!("price of item {item} {key}");
                for (chapter, value) in Self::chapters(value, &what, false)?.into_iter().enumerate() {
                    let Some(number) = value.as_i64().filter(|number| (0..=i32::MAX as i64).contains(number)) else {
                        bail!("{what} is a whole number from 0");
                    };
                    // a value the export shows unchanged changes nothing, in either set of files
                    let shown = places[chapter].and_then(|at| self.values(chapter, at).get(arg).map(Arg::int));
                    if shown == Some(number as i32) {
                        continue;
                    }
                    for file in [chapter, chapter + CHAPTERS].into_iter().filter(|&file| file < files) {
                        let Some(at) = places[file] else {
                            continue;
                        };
                        if self.values(file, at).get(arg).map(Arg::int) == Some(number as i32) {
                            continue;
                        }
                        if self.scripts[file].statements[at].args().len() <= arg {
                            bail!("{what}: the price line in chapter {} has no {key}", chapter + 1);
                        }
                        out.push(Edit {
                            file,
                            statement: at,
                            arg,
                            to: To::Arg(cfg_text::int_text(number)),
                            what: format!("{what} (chapter {})", chapter + 1),
                            module: module.to_string(),
                        });
                    }
                }
            }
        }
        for record in list("shops")? {
            let Some(shop) = record.get("shop").and_then(Value::as_i64) else {
                bail!("each shop names its id: {{\"shop\": 0, \"items\": [...]}}");
            };
            let none = vec![None; files];
            let places = match self.shops.get(&(shop as i32)) {
                Some(places) => places,
                None if (0..=i16::MAX as i64).contains(&shop) => {
                    if !record.contains_key("items") {
                        bail!("new shop {shop}: a new shop gives its \"items\"");
                    }
                    &none
                }
                None => bail!("shop {shop}: a shop number is 0 to {} (CShop::shop_id)", i16::MAX),
            };
            for (key, value) in record {
                match key.as_str() {
                    "shop" | "currency" | "names" => continue,
                    "items" => {}
                    _ => bail!("shop {shop}: \"{key}\" is not a field (items)"),
                }
                let what = format!("shop {shop} items");
                for (chapter, value) in Self::chapters(value, &what, true)?.into_iter().enumerate() {
                    let Some(items) = value.as_array() else {
                        bail!("{what} is a list of item numbers");
                    };
                    let mut numbers = Vec::new();
                    for item in items {
                        let Some(number) = item.as_i64().filter(|number| (1..PRICE_SLOTS).contains(number)) else {
                            bail!("{what}: {item} is not an item number");
                        };
                        if !self.prices.get(&(number as i32)).is_some_and(|places| places[chapter].is_some())
                            && !priced.contains(&(number as i32))
                        {
                            bail!(
                                "{what}: item {number} has no price in chapter {}; give it one under \"prices\"",
                                chapter + 1
                            );
                        }
                        numbers.push(number as i32);
                    }
                    if numbers.len() > SHOP_ITEM_MAX {
                        bail!("{what}: {} goods; a shop lists at most {SHOP_ITEM_MAX}", numbers.len());
                    }
                    let shown: Option<Vec<i32>> =
                        places[chapter].map(|at| self.values(chapter, at)[1..].iter().map(Arg::int).collect());
                    if shown.as_ref() == Some(&numbers) {
                        continue;
                    }
                    for file in [chapter, chapter + CHAPTERS].into_iter().filter(|&file| file < files) {
                        let Some(at) = places[file] else {
                            // a shop the file lacks: a SHOP line of its own
                            let mut args = vec![cfg_text::int_text(shop)];
                            args.extend(numbers.iter().map(|&number| cfg_text::int_text(number as i64)));
                            let what = format!("new shop {shop} (chapter {})", chapter + 1);
                            out.push(self.added(file, "SHOP", args, what, module)?);
                            continue;
                        };
                        let old: Vec<i32> = self.values(file, at)[1..].iter().map(Arg::int).collect();
                        if old == numbers {
                            continue;
                        }
                        out.push(Edit {
                            file,
                            statement: at,
                            arg: 1,
                            to: To::Rest(numbers.iter().map(|&number| cfg_text::int_text(number as i64)).collect()),
                            what: format!("{what} (chapter {})", chapter + 1),
                            module: module.to_string(),
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// The files with the edits made, by [`files`].
    pub fn apply(&self, edits: &[Edit]) -> Result<Vec<Vec<u8>>> {
        cfg_text::apply_edits(&self.scripts, edits)
    }

    pub fn write(&self) -> Vec<Vec<u8>> {
        self.scripts.iter().map(Script::write).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Vec<Vec<u8>> {
        (0..2 * CHAPTERS)
            .map(|file| {
                let sell = if file % CHAPTERS >= 4 { 300 } else { 225 };
                let goods = if file % CHAPTERS == 7 { "1,11,12" } else { "1,11" };
                format!("PRICE 1,100,50;\r\nPRICE 11,450,{sell};\r\nPRICE 12,640,320;\r\n//shop\r\nSHOP 0,{goods};\r\nSHOP 33;\r\n")
                    .into_bytes()
            })
            .collect()
    }

    #[test]
    fn prices_and_goods_read_by_chapter() {
        let shops = Shops::read(&sample(), Some(b"MES_SYS 11,\"Swan\";\r\n")).unwrap();
        let json = shops.to_json();
        assert_eq!(
            json["prices"][1],
            json!({"item": 11, "name": "Swan", "buy": 450, "sell": [225, 225, 225, 225, 300, 300, 300, 300]})
        );
        let goods = &json["shops"][0];
        assert_eq!(goods["currency"], "money");
        assert_eq!(goods["items"][0], json!([1, 11]));
        assert_eq!(goods["items"][7], json!([1, 11, 12]));
        assert_eq!(goods["names"][0], json!(["", "Swan"]));
        assert_eq!(json["shops"][1]["items"], json!([]));
        assert!(shops.changes(&json, "m").unwrap().is_empty());
        assert_eq!(shops.write(), sample());
    }

    #[test]
    fn edits_reach_every_chapter_and_language() {
        let shops = Shops::read(&sample(), None).unwrap();
        let edit = json!({"prices": [{"item": 11, "buy": 500}], "shops": [{"shop": 33, "items": [12, 1]}]});
        let files = shops.apply(&shops.changes(&edit, "m").unwrap()).unwrap();
        for file in &files {
            let text = String::from_utf8_lossy(file);
            assert!(text.contains("PRICE 11,500,") && text.contains("SHOP 33,12,1;\r\n"), "{text}");
        }
        let edit = json!({"prices": [{"item": 11, "sell": [1, 2, 3, 4, 5, 6, 7, 8]}]});
        let files = shops.apply(&shops.changes(&edit, "m").unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&files[CHAPTERS + 2]).contains("PRICE 11,450,3;"));
        let refused = |edit: Value| shops.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(json!({"shops": [{"shop": 0, "items": [99]}]})).contains("no price"));
        assert!(refused(json!({"shops": [{"shop": 5}]})).contains("gives its \"items\""));
        assert!(refused(json!({"shops": [{"shop": 40000, "items": []}]})).contains("shop number"));
        assert!(refused(json!({"prices": [{"item": 429, "buy": 1}]})).contains("\"buy\" and \"sell\""));
        assert!(refused(json!({"prices": [{"item": 512, "buy": 1, "sell": 0}]})).contains("1 to 511"));
    }

    #[test]
    fn new_prices_and_shops_are_new_lines() {
        let shops = Shops::read(&sample(), None).unwrap();
        let price = json!({"prices": [{"item": 429, "buy": 1, "sell": 0}]});
        let sold = json!({"shops": [{"shop": 0, "items": [1, 429]}, {"shop": 40, "items": [429]}]});
        // the price in one mod, the goods in another
        let edits = shops.changes_all(&[("a".into(), price.clone()), ("b".into(), sold)]).unwrap();
        let files = shops.apply(&edits).unwrap();
        for file in &files {
            let text = String::from_utf8_lossy(file);
            assert!(
                text.contains(
                    "PRICE 12,640,320;\r\nPRICE 429,1,0;\r\n//shop\r\nSHOP 0,1,429;\r\nSHOP 33;\r\nSHOP 40,429;\r\n"
                ),
                "{text}"
            );
        }
        let again = Shops::read(&files, None).unwrap().to_json();
        assert_eq!(again["shops"][2], json!({"shop": 40, "currency": "money", "items": [429]}));
        // one price given twice alike is one line; given two ways, refused
        let twice = shops.changes_all(&[("a".into(), price.clone()), ("b".into(), price)]).unwrap();
        assert_eq!(String::from_utf8_lossy(&shops.apply(&twice).unwrap()[0]).matches("PRICE 429").count(), 1);
        let other = json!({"prices": [{"item": 429, "buy": 2, "sell": 0}]});
        let clash = shops
            .changes_all(&[("a".into(), json!({"prices": [{"item": 429, "buy": 1, "sell": 0}]})), ("b".into(), other)]);
        assert!(shops.apply(&clash.unwrap()).unwrap_err().to_string().contains("mod a and mod b"));
        let refused = |edit: Value| shops.changes(&edit, "m").unwrap_err().to_string();
        assert!(refused(json!({"prices": [{"item": 11, "buy": [1, 2]}]})).contains("list of 8"));
        assert!(refused(json!({"prices": [{"item": 11, "buy": -1}]})).contains("from 0"));
        assert!(refused(json!({"shops": [{"shop": 0, "items": vec![1; 64]}]})).contains("at most 63"));
    }

    /// The shop lists of an extracted disc (C2S_CORPUS) round-trip, and their JSON changes nothing.
    #[test]
    fn corpus_shops_round_trip() {
        let Some(corpus) = std::env::var_os("C2S_CORPUS") else {
            return;
        };
        let menu = std::path::Path::new(&corpus).join(FOLDER);
        let files: Vec<Vec<u8>> = super::files().iter().map(|file| std::fs::read(menu.join(file)).unwrap()).collect();
        let names = std::fs::read(menu.join("cfg7").join(crate::item_data::NAMES)).ok();
        let shops = Shops::read(&files, names.as_deref()).unwrap();
        assert_eq!(shops.write(), files);
        let json: Value = serde_json::from_str(&crate::item_data::json_text(&shops.to_json())).unwrap();
        assert!(shops.changes(&json, "m").unwrap().is_empty());
        for (item, places) in &shops.prices {
            for (file, at) in places.iter().enumerate() {
                if let Some(at) = at {
                    for piece in &shops.scripts[file].statements[*at].args()[..3] {
                        assert_eq!(cfg_text::int_text(piece.value().int() as i64), piece.text, "item {item}");
                    }
                }
            }
        }
        eprintln!("{} prices and {} shops in {} files round-trip", shops.prices.len(), shops.shops.len(), files.len());
    }
}
