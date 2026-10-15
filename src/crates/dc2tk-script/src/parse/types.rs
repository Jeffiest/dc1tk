//! Types: declaration specifiers, structs and unions, declarators and type names.

use super::*;

// ---- types ---------------------------------------------------------------------------------

pub(super) const STORAGE: [&str; 5] = ["static", "extern", "auto", "register", "typedef"];
pub(super) const QUALIFIERS: [&str; 2] = ["const", "volatile"];
pub(super) const INTEGER_WORDS: [&str; 6] = ["char", "short", "int", "long", "signed", "unsigned"];
/// The integer types besides int, as declarations name them (char is signed; long is int).
pub const INTEGER_TYPES: [&str; 5] = ["unsigned int", "short", "unsigned short", "char", "unsigned char"];

/// Whether the token offset ahead starts a type: a keyword of one, a typedef of this file, or a
/// header's typedef followed by what only a declaration or a cast has there.
pub(super) fn starts_type(state: &State, offset: usize) -> bool {
    let token = state.token(offset);
    token.kind == Kind::Name && (type_word(state, token.text()) || header_type_ahead(state, offset))
}

/// A typedef of an included header (size_t, a value kind) used as a type word: one the file has
/// not declared as anything else (a variable, an enumerator or a constant hides it, as in C). The
/// SDK's value kinds have names scripts use for variables too, so the name counts as a type only
/// where it is followed by a name or * (a declaration) or ) (a cast, sizeof).
pub(super) fn header_type_ahead(state: &State, offset: usize) -> bool {
    header_type(state, state.token(offset).text(), state.token(offset + 1))
}

/// header_type_ahead of a name and the token after it.
pub(super) fn header_type(state: &State, text: &str, next: &Token) -> bool {
    if !state.header_kinds.contains_key(text)
        || state.variable(&state.storage_name(text)).is_some()
        || state.enumerators.contains_key(text)
        || state.constants.contains_key(text)
    {
        return false;
    }
    next.kind == Kind::Name || (next.kind == Kind::Op && ["*", ")"].contains(&next.text()))
}

pub(super) fn type_word(state: &State, text: &str) -> bool {
    STORAGE.contains(&text)
        || QUALIFIERS.contains(&text)
        || INTEGER_WORDS.contains(&text)
        || ["float", "string", "void", "enum", "struct", "union"].contains(&text)
        || state.kinds.contains_key(text)
        || state.types.contains_key(text)
}

/// What a declaration's specifiers say: the type, the storage class, and the qualifiers.
#[derive(Clone, Debug, Default)]
pub(super) struct Spec {
    pub(super) kind: String,
    pub(super) storage: Option<String>,
    pub(super) constant: bool,
    pub(super) volatile: bool,
    /// A struct or union, or a typedef of a type beyond the scalars.
    pub(super) ty: Option<Ty>,
}

/// The type the integer keywords of a declaration name, as C 3.5.2 lists their combinations.
pub(super) fn integer_type(words: &[String]) -> std::result::Result<&'static str, String> {
    let count = |word: &str| words.iter().filter(|known| *known == word).count();
    let shown = || words.join(" ");
    if count("long") > 1 {
        return Err("long long is C99, and the VM's integers are 32 bits".into());
    }
    if ["char", "short", "int", "signed", "unsigned"].iter().any(|word| count(word) > 1)
        || (count("signed") > 0 && count("unsigned") > 0)
    {
        return Err(format!("{} is not a type", shown()));
    }
    let unsigned = count("unsigned") > 0;
    if count("char") > 0 {
        if count("short") + count("long") + count("int") > 0 {
            return Err(format!("{} is not a type", shown()));
        }
        return Ok(if unsigned { "unsigned char" } else { "char" });
    }
    if count("short") > 0 {
        if count("long") > 0 {
            return Err(format!("{} is not a type", shown()));
        }
        return Ok(if unsigned { "unsigned short" } else { "short" });
    }
    Ok(if unsigned { "unsigned int" } else { "int" })
}

/// The specifiers read so far: the integer keywords, and the base type other words give.
#[derive(Default)]
pub(super) struct Specifiers {
    pub(super) spec: Spec,
    pub(super) words: Vec<String>,
    pub(super) base: Option<String>,
    pub(super) any: bool,
}

/// A declaration's specifiers. header_kinds: an unknown name followed by a name or * is a value
/// kind from a header (bgm_file track;), which sdk::resolve reads as its base type.
pub(super) fn specifiers(state: &mut State, header_kinds: bool) -> Result<Spec> {
    let mut found = Specifiers::default();
    while state.token(0).kind == Kind::Name {
        let tagged_ahead = ["struct", "union", "enum"].contains(&state.token(0).text());
        if tagged_ahead && found.base.is_none() && found.words.is_empty() {
            // read apart from the other words: a struct's members nest specifiers in it
            let word = state.take();
            tagged(state, &mut found, word.text())?;
        } else if !specifier(state, &mut found, header_kinds)? {
            break;
        }
        found.any = true;
    }
    if !found.any {
        let found = state.shown(state.token(0))?;
        return state.fail(format!("expected a type, found {found}"));
    }
    let Specifiers { mut spec, words, base, .. } = found;
    spec.kind = match base {
        Some(base) if words.is_empty() => base,
        Some(base) => return state.fail(format!("{} {base} is not a type", words.join(" "))),
        // C89's implicit int: static x; const y = 1;
        None => integer_type(&words).or_else(|reason| state.fail(reason))?.to_string(),
    };
    Ok(spec)
}

/// Reads the specifier word ahead into found: false, reading nothing, when it is not one (the
/// declarator starts there).
pub(super) fn specifier(state: &mut State, found: &mut Specifiers, header_kinds: bool) -> Result<bool> {
    let text = state.token(0).text().to_string();
    if STORAGE.contains(&text.as_str()) {
        if found.spec.storage.is_some() {
            return state.fail("a declaration has one storage class");
        }
        found.spec.storage = Some(text);
    } else if text == "const" {
        found.spec.constant = true;
    } else if text == "volatile" {
        found.spec.volatile = true;
    } else if INTEGER_WORDS.contains(&text.as_str()) && found.base.is_none() {
        found.words.push(text);
    } else if found.base.is_some() || !found.words.is_empty() {
        return Ok(false);
    } else if ["float", "string", "void"].contains(&text.as_str()) || state.kinds.contains_key(&text) {
        found.base = Some(text);
    } else if let Some(ty) = state.types.get(&text).cloned() {
        found.spec.ty = Some(ty);
        found.base = Some(text);
    } else if header_type_ahead(state, 0)
        || (header_kinds
            && (state.token(1).kind == Kind::Name || (state.token(1).kind == Kind::Op && state.token(1).text() == "*")))
        // an unnamed parameter of a header's type: int f(size_t, size_t);
        || (header_kinds
            && state.header_kinds.contains_key(&text)
            && state.token(1).kind == Kind::Op
            && [",", "["].contains(&state.token(1).text()))
    {
        found.base = Some(text);
    } else {
        return Ok(false);
    }
    state.take();
    Ok(true)
}

/// struct, union or enum, after its keyword: the type it names (or defines here).
pub(super) fn tagged(state: &mut State, found: &mut Specifiers, word: &str) -> Result<()> {
    if word == "enum" {
        return enum_specifier(state, found);
    }
    let tag = struct_specifier(state, word == "union")?;
    found.tagged(word, tag);
    Ok(())
}

/// enum NAME, or enum NAME { ... } or enum { ... } defined here, after its keyword.
pub(super) fn enum_specifier(state: &mut State, found: &mut Specifiers) -> Result<()> {
    let name = if state.at("{", 0) { None } else { Some(state.name()?) };
    if state.at("{", 0) {
        // the enum is defined here; its members are the file's constants
        let members = enumerators(state, name.as_deref())?;
        state.constants_found.extend(members);
    }
    found.base = Some(name.map_or("int".to_string(), |name| format!("enum {name}")));
    Ok(())
}

impl Specifiers {
    /// A struct or union a declaration's specifiers name.
    pub(super) fn tagged(&mut self, word: &str, tag: String) {
        self.base = Some(format!("{word} {tag}"));
        self.spec.ty = Some(Ty::Struct(tag));
    }
}

/// struct TAG { members } or union TAG { ... } after its keyword (or TAG alone, naming one): the
/// tag, a made-up one for an unnamed struct.
pub(super) fn struct_specifier(state: &mut State, union: bool) -> Result<String> {
    state.nested(|state| {
        let tag = struct_tag(state, union)?;
        if !state.at("{", 0) {
            return Ok(declared_struct(state, tag, union));
        }
        let composite = struct_members(state, union, &tag)?;
        defined(state, tag, composite)
    })
}

/// A struct declared here, defined (or not) elsewhere: an incomplete type until then.
pub(super) fn declared_struct(state: &mut State, tag: String, union: bool) -> String {
    state.structs.entry(tag.clone()).or_insert(Composite::new(union, Vec::new()));
    tag
}

/// A struct or union as a message names it: struct T.
pub(super) fn struct_shown(union: bool, tag: &str) -> String {
    format!("{} {}", if union { "union" } else { "struct" }, ctype::tag_shown(tag))
}

/// A struct's definition, kept unless the struct is defined differently already.
pub(super) fn defined(state: &mut State, tag: String, composite: Composite) -> Result<String> {
    if state.structs.get(&tag).is_some_and(|known| !known.members.is_empty() && *known != composite) {
        return state.fail(format!("{} is defined twice", struct_shown(composite.union, &tag)));
    }
    state.structs.insert(tag.clone(), composite);
    Ok(tag)
}

/// The tag of a struct or union: its name's (a struct of the block's own, where a block defines
/// or declares it), or a made-up one for an unnamed struct.
pub(super) fn struct_tag(state: &mut State, union: bool) -> Result<String> {
    let tag = if state.token(0).kind == Kind::Name {
        let name = state.name()?;
        if state.at("{", 0) || state.at(";", 0) {
            // defined or declared alone here: in a block, a struct of the block's
            state.declared_tag(&name)
        } else {
            state.tag(&name)
        }
    } else if state.at("{", 0) {
        state.anonymous += 1;
        format!("__anonymous_{}_{}", state.file_key, state.anonymous)
    } else {
        let found = state.shown(state.token(0))?;
        let word = if union { "union" } else { "struct" };
        return state.fail(format!("expected the name of a {word}, found {found}"));
    };
    if state.structs.get(&tag).is_some_and(|known| known.union != union) {
        let (word, other) = if union { ("union", "struct") } else { ("struct", "union") };
        return state.fail(format!("{} is a {other}, not a {word}", ctype::tag_shown(&tag)));
    }
    Ok(tag)
}

/// A struct's or union's { members }.
pub(super) fn struct_members(state: &mut State, union: bool, tag: &str) -> Result<Composite> {
    state.take();
    let shown = struct_shown(union, tag);
    let mut members = Members { union, shown, members: Vec::new(), bits: Vec::new(), used: None };
    while !state.at("}", 0) {
        member_declaration(state, &mut members)?;
    }
    state.expect("}")?;
    members.composite(state)
}

/// A declaration of members, which nests what its type defines (a struct inside a struct).
pub(super) fn member_declaration(state: &mut State, members: &mut Members) -> Result<()> {
    state.nested(|state| {
        let spec = specifiers(state, true)?;
        member_declarators(state, members, spec)
    })
}

/// The members a declaration's declarators give, after its specifiers.
pub(super) fn member_declarators(state: &mut State, members: &mut Members, spec: Spec) -> Result<()> {
    if spec.storage.is_some() {
        return state.fail(format!("a member of {} has no storage class", members.shown));
    }
    let base = base_ty(state, &spec)?;
    loop {
        let (name, ty) = declarator(state, base.clone())?;
        members.add(state, name, ty)?;
        if !state.at(",", 0) {
            break;
        }
        state.take();
    }
    state.expect(";")?;
    Ok(())
}

/// A struct's members as they are read: each takes whole slots, but bit-fields share them.
pub(super) struct Members {
    pub(super) union: bool,
    /// The struct as messages name it.
    pub(super) shown: String,
    pub(super) members: Vec<(String, Ty)>,
    pub(super) bits: Vec<Option<ctype::Bits>>,
    /// The bits of the slot bit-fields are filling the fields so far have used, while one is.
    pub(super) used: Option<i64>,
}

impl Members {
    pub(super) fn composite(self, state: &State) -> Result<Composite> {
        if self.members.is_empty() {
            return state.fail(format!("{} needs a member", self.shown));
        }
        let bits = if self.bits.iter().any(Option::is_some) { self.bits } else { Vec::new() };
        Ok(Composite { union: self.union, members: self.members, bits })
    }

    /// A member's declarator read: a bit-field when a width follows it.
    pub(super) fn add(&mut self, state: &mut State, name: String, ty: Ty) -> Result<()> {
        if !name.is_empty() && self.members.iter().any(|(known, _)| *known == name) {
            return state.fail(format!("{} has two members named {name}", self.shown));
        }
        if state.at(":", 0) {
            state.take();
            let width = constant_expression(state)?;
            return self.bit_field(state, name, ty, width);
        }
        if name.is_empty() {
            return state.fail(format!("a member of {} needs a name", self.shown));
        }
        complete(state, &name, &ty)?;
        self.members.push((name, ty));
        self.bits.push(None);
        self.used = None;
        Ok(())
    }

    pub(super) fn bit_field(&mut self, state: &State, name: String, ty: Ty, width: i64) -> Result<()> {
        let kind = match &ty {
            Ty::Scalar(kind) => state.base_kind(kind),
            _ => String::new(),
        };
        if kind != "int" && kind != "unsigned int" {
            return state
                .fail(format!("bit-field {name} is of {}: a bit-field is an int or an unsigned int", ty.shown()));
        }
        if !(0..=32).contains(&width) {
            return state.fail(format!("bit-field {name} has {width} bits; an int has 32"));
        }
        match width {
            0 if !name.is_empty() => {
                return state.fail(format!("bit-field {name} has 0 bits; only an unnamed one may"))
            }
            // :0 ends the slot: the next bit-field starts another
            0 => self.used = None,
            32 => {
                // the whole slot, an int of its own
                self.members.push((name, ty));
                self.bits.push(None);
                self.used = None;
            }
            _ => {
                let offset = match self.used {
                    Some(taken) if !self.union && taken + width <= 32 => taken,
                    _ => 0,
                };
                self.members.push((name, ty));
                self.bits.push(Some(ctype::Bits { offset, width }));
                self.used = Some(offset + width);
            }
        }
        Ok(())
    }
}

/// Refuses an object of a type with no size: void, a function, a struct not defined, an array
/// whose size nothing gave.
pub(super) fn complete(state: &State, name: &str, ty: &Ty) -> Result<()> {
    match ty {
        Ty::Void => state.fail(format!("{name} cannot be void")),
        Ty::Function(..) => state.fail(format!("{name} is a function; declare functions at file scope")),
        Ty::Struct(tag) if state.structs.get(tag).is_none_or(|found| found.members.is_empty()) => {
            state.fail(format!("{name} is of {}, which is not defined", ty.shown()))
        }
        Ty::Array(_, -1) => state.fail(format!("{name}[] needs a size or a list of values")),
        Ty::Array(of, _) => complete(state, name, of),
        _ => Ok(()),
    }
}

/// The type specifiers give: a struct or typedef'd type as it is, void, or a scalar by its base
/// type (a typedef's, int for an enum; a header's value kind as it is named).
pub(super) fn base_ty(state: &State, spec: &Spec) -> Result<Ty> {
    if let Some(ty) = &spec.ty {
        return Ok(ty.clone());
    }
    Ok(match spec.kind.as_str() {
        "void" => Ty::Void,
        kind if kind.starts_with("enum ") => ctype::int(),
        kind => Ty::Scalar(state.base_kind(kind)),
    })
}

/// A declarator, as C 3.5.4 reads one: *s (with their qualifiers), a name or a declarator in
/// parentheses, then [N]s and (parameters). The name ("" in a type name) and the type it gives
/// the name, built from base inside out: int *a[3] is an array of pointers, int (*f)(int) a pointer
/// to a function.
pub(super) fn declarator(state: &mut State, base: Ty) -> Result<(String, Ty)> {
    state.nested(|state| {
        let mut ty = base;
        while state.at("*", 0) {
            state.take();
            while state.at("const", 0) || state.at("volatile", 0) {
                state.take();
            }
            ty = ctype::pointer(ty);
        }
        let nested = state.at("(", 0)
            && (state.at("*", 1)
                || state.at("(", 1)
                || state.at("[", 1)
                || (state.token(1).kind == Kind::Name && !type_word(state, state.token(1).text())));
        if nested {
            // what follows the parentheses applies first: read it, then the declarator inside
            state.take();
            let inner = state.pos;
            let mut depth = 1;
            while depth > 0 {
                if state.token(0).kind == Kind::Eof {
                    return state.fail("expected ')'");
                }
                let token = state.take();
                if token.kind == Kind::Op {
                    depth += match token.text() {
                        "(" => 1,
                        ")" => -1,
                        _ => 0,
                    };
                }
            }
            let ty = suffixes(state, ty)?;
            let end = state.pos;
            state.pos = inner;
            let (name, ty) = declarator(state, ty)?;
            state.expect(")")?;
            state.pos = end;
            return Ok((name, ty));
        }
        let named = state.token(0).kind == Kind::Name && !type_word(state, state.token(0).text());
        let name = if named { state.name()? } else { String::new() };
        Ok((name, suffixes(state, ty)?))
    })
}

/// The [N]s and (parameters) after a declarator's name, applied to ty from the last in.
pub(super) fn suffixes(state: &mut State, ty: Ty) -> Result<Ty> {
    let mut found: Vec<std::result::Result<i64, (Vec<Ty>, bool)>> = Vec::new();
    loop {
        if state.at("[", 0) {
            state.take();
            let size = if state.at("]", 0) { -1 } else { constant_expression(state)? };
            state.expect("]")?;
            if size == 0 || size < -1 || (size == -1 && !found.is_empty()) {
                return state.fail("an array needs a size of at least 1");
            }
            found.push(Ok(size));
        } else if state.at("(", 0) {
            state.take();
            found.push(Err(parameter_types(state)?));
        } else {
            break;
        }
    }
    let mut ty = ty;
    for suffix in found.into_iter().rev() {
        ty = match suffix {
            Ok(size) => Ty::Array(Box::new(ty), size),
            Err((params, variadic)) => Ty::Function(Box::new(ty), params, variadic),
        };
    }
    Ok(ty)
}

/// A parameter's type as the function sees it: an array is a pointer to its first element, a
/// function a pointer to it.
pub(super) fn adjusted(ty: Ty) -> Ty {
    match ty {
        Ty::Array(of, _) => Ty::Pointer(of),
        function @ Ty::Function(..) => ctype::pointer(function),
        other => other,
    }
}

/// The parameter types of a function type, after its (: names allowed and ignored.
pub(super) fn parameter_types(state: &mut State) -> Result<(Vec<Ty>, bool)> {
    let mut params = Vec::new();
    let mut variadic = false;
    if !no_parameters(state) {
        loop {
            if state.at("...", 0) {
                state.take();
                variadic = true;
                break;
            }
            let spec = specifiers(state, true)?;
            let base = base_ty(state, &spec)?;
            let (_, ty) = declarator(state, base)?;
            params.push(adjusted(ty));
            if !state.at(",", 0) {
                break;
            }
            state.take();
        }
    }
    state.expect(")")?;
    Ok((params, variadic))
}

/// Whether the declarator ahead needs C's full declarators (declarator), not the forms the
/// decompiler writes (x, *x, x[N]...): a struct or typedef'd type, void *, pointers to pointers,
/// parentheses, an array of pointers.
pub(super) fn complex_ahead(state: &State, spec: &Spec) -> bool {
    if spec.ty.is_some() {
        return true;
    }
    let mut offset = 0;
    let mut stars = 0;
    while state.at("*", offset) || state.at("const", offset) || state.at("volatile", offset) {
        stars += usize::from(state.at("*", offset));
        offset += 1;
    }
    (spec.kind == "void" && stars > 0)
        || stars > 1
        || state.at("(", offset)
        || (stars == 1 && state.token(offset).kind == Kind::Name && state.at("[", offset + 1))
}

/// A type name in a cast, sizeof or offsetof: specifiers and an abstract declarator.
pub(super) fn abstract_type(state: &mut State) -> Result<Ty> {
    let spec = specifiers(state, false)?;
    if spec.storage.is_some() {
        return state.fail("a type name has no storage class");
    }
    let base = base_ty(state, &spec)?;
    let (name, ty) = declarator(state, base)?;
    if !name.is_empty() {
        return state.fail(format!("a type name has no name, found {name}"));
    }
    Ok(ty)
}
