//! IBus serializable objects as D-Bus values.
//!
//! libibus sends every `IBusSerializable` as a struct of its type name, an
//! `a{sv}` of attachments, then the type's own fields, each nested object
//! wrapped in a variant (`ibus_serializable_serialize_object` and the
//! `*_serialize` functions of `ibustext.c`, `ibusattrlist.c`, ...).

use std::collections::HashMap;

use serde::Serialize;
use zbus::wire::{Signature, Type};
use zbus::{OwnedVariant, Variant};

/// A value sent as a D-Bus variant (`v`).
#[derive(Debug)]
pub struct V<T>(pub T);

impl<T: Type + Serialize> Serialize for V<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        zbus::as_variant::serialize(&self.0, serializer)
    }
}

impl<T> Type for V<T> {
    const SIGNATURE: &'static Signature = &Signature::Variant;
}

/// The attachments every serializable carries; karukan sends none.
type Attachments = HashMap<String, OwnedVariant>;

/// `IBusAttrType` (ibusattribute.h).
pub const ATTR_TYPE_UNDERLINE: u32 = 1;
pub const ATTR_TYPE_FOREGROUND: u32 = 2;
pub const ATTR_TYPE_BACKGROUND: u32 = 3;
/// `IBusAttrUnderline`.
pub const ATTR_UNDERLINE_SINGLE: u32 = 1;
pub const ATTR_UNDERLINE_DOUBLE: u32 = 2;

/// `IBusOrientation`.
const ORIENTATION_VERTICAL: i32 = 1;

/// `IBusAttribute`: a style over `[start, end)`, counted in characters.
#[derive(Debug, Serialize, Type)]
pub struct Attribute {
    type_name: &'static str,
    attachments: Attachments,
    kind: u32,
    value: u32,
    start: u32,
    end: u32,
}

impl Attribute {
    pub fn new(kind: u32, value: u32, start: usize, end: usize) -> Self {
        Self {
            type_name: "IBusAttribute",
            attachments: Attachments::new(),
            kind,
            value,
            start: start as u32,
            end: end as u32,
        }
    }

    #[cfg(test)]
    pub fn kind(&self) -> u32 {
        self.kind
    }
}

/// `IBusAttrList`.
#[derive(Debug, Serialize, Type)]
pub struct AttrList {
    type_name: &'static str,
    attachments: Attachments,
    attributes: Vec<V<Attribute>>,
}

/// `IBusText`.
#[derive(Debug, Serialize, Type)]
pub struct Text {
    type_name: &'static str,
    attachments: Attachments,
    text: String,
    attrs: V<AttrList>,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_attributes(text, Vec::new())
    }

    pub fn with_attributes(text: impl Into<String>, attributes: Vec<Attribute>) -> Self {
        Self {
            type_name: "IBusText",
            attachments: Attachments::new(),
            text: text.into(),
            attrs: V(AttrList {
                type_name: "IBusAttrList",
                attachments: Attachments::new(),
                attributes: attributes.into_iter().map(V).collect(),
            }),
        }
    }

    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub fn attributes(&self) -> impl Iterator<Item = &Attribute> {
        self.attrs.0.attributes.iter().map(|a| &a.0)
    }
}

/// The plain string of an `IBusText` received as a variant
/// (`SetSurroundingText`): its third field.
pub fn text_of(variant: &Variant<'_>) -> Option<String> {
    match variant {
        Variant::Structure(s) => match s.fields().get(2) {
            Some(Variant::Str(text)) => Some(text.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// `IBusLookupTable`. karukan pages candidates itself, so a table holds
/// the shown page only, with the cursor inside it.
#[derive(Debug, Serialize, Type)]
pub struct LookupTable {
    type_name: &'static str,
    attachments: Attachments,
    page_size: u32,
    cursor_pos: u32,
    cursor_visible: bool,
    round: bool,
    orientation: i32,
    candidates: Vec<V<Text>>,
    labels: Vec<V<Text>>,
}

impl LookupTable {
    pub fn new(page_size: usize, candidates: Vec<Text>, cursor: usize) -> Self {
        let labels = (1..=candidates.len())
            .map(|n| V(Text::new(n.to_string())))
            .collect();
        Self {
            type_name: "IBusLookupTable",
            attachments: Attachments::new(),
            page_size: page_size as u32,
            cursor_pos: cursor as u32,
            cursor_visible: true,
            round: false,
            orientation: ORIENTATION_VERTICAL,
            candidates: candidates.into_iter().map(V).collect(),
            labels,
        }
    }

    #[cfg(test)]
    pub fn candidates(&self) -> impl Iterator<Item = &str> {
        self.candidates.iter().map(|c| c.0.text())
    }

    #[cfg(test)]
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.labels.iter().map(|c| c.0.text())
    }

    #[cfg(test)]
    pub fn cursor_pos(&self) -> u32 {
        self.cursor_pos
    }
}

/// `IBusEngineDesc`, in the order `ibus_engine_desc_serialize` writes it.
#[derive(Debug, Serialize, Type)]
pub struct EngineDesc {
    type_name: &'static str,
    attachments: Attachments,
    pub name: &'static str,
    pub longname: &'static str,
    pub description: &'static str,
    pub language: &'static str,
    pub license: &'static str,
    pub author: &'static str,
    pub icon: &'static str,
    pub layout: &'static str,
    pub rank: u32,
    pub hotkeys: &'static str,
    pub symbol: &'static str,
    pub setup: &'static str,
    pub layout_variant: &'static str,
    pub layout_option: &'static str,
    pub version: &'static str,
    pub textdomain: &'static str,
    pub icon_prop_key: &'static str,
}

/// `IBusComponent`, in the order `ibus_component_serialize` writes it.
#[derive(Debug, Serialize, Type)]
pub struct Component {
    type_name: &'static str,
    attachments: Attachments,
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub author: &'static str,
    pub homepage: &'static str,
    pub exec: String,
    pub textdomain: &'static str,
    /// `IBusObservedPath`s; karukan watches no files.
    observed_paths: Vec<OwnedVariant>,
    engines: Vec<V<EngineDesc>>,
}

pub const COMPONENT_NAME: &str = "org.freedesktop.IBus.Karukan";
pub const ENGINE_NAME: &str = "karukan";
/// Where the installed component XML points `exec` (docs/ibus.md).
pub const INSTALLED_EXEC: &str = "/usr/libexec/karukan-ibus --ibus";

/// The workspace version, as the fcitx5 addon's `Version=` carries it.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The karukan component, starting the engine with `exec`.
pub fn component(exec: String) -> Component {
    const LICENSE: &str = "MIT OR Apache-2.0";
    const AUTHOR: &str = "togatoga";
    const DESCRIPTION: &str = "Karukan Japanese Input Method";
    Component {
        type_name: "IBusComponent",
        attachments: Attachments::new(),
        name: COMPONENT_NAME,
        description: DESCRIPTION,
        version: VERSION,
        license: LICENSE,
        author: AUTHOR,
        homepage: "https://github.com/togatoga/karukan",
        exec,
        textdomain: "",
        observed_paths: Vec::new(),
        engines: vec![V(EngineDesc {
            type_name: "IBusEngineDesc",
            attachments: Attachments::new(),
            name: ENGINE_NAME,
            longname: "Karukan",
            description: DESCRIPTION,
            language: "ja",
            license: LICENSE,
            author: AUTHOR,
            icon: "",
            // Keep the user's keyboard layout (US, JIS, ...).
            layout: "default",
            rank: 0,
            hotkeys: "",
            symbol: "あ",
            setup: "",
            layout_variant: "",
            layout_option: "",
            version: VERSION,
            textdomain: "",
            icon_prop_key: "",
        })],
    }
}

impl Component {
    pub fn engines(&self) -> impl Iterator<Item = &EngineDesc> {
        self.engines.iter().map(|e| &e.0)
    }

    /// The component file ibus-daemon reads from its component directory.
    pub fn to_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<component>\n");
        let fields = [
            ("name", self.name),
            ("description", self.description),
            ("exec", self.exec.as_str()),
            ("version", self.version),
            ("license", self.license),
            ("author", self.author),
            ("homepage", self.homepage),
            ("textdomain", self.textdomain),
        ];
        push_elements(&mut xml, "  ", &fields);
        xml.push_str("  <engines>\n");
        for engine in self.engines() {
            xml.push_str("    <engine>\n");
            let rank = engine.rank.to_string();
            let fields = [
                ("name", engine.name),
                ("longname", engine.longname),
                ("description", engine.description),
                ("language", engine.language),
                ("license", engine.license),
                ("author", engine.author),
                ("layout", engine.layout),
                ("rank", rank.as_str()),
                ("symbol", engine.symbol),
                ("version", engine.version),
            ];
            push_elements(&mut xml, "      ", &fields);
            xml.push_str("    </engine>\n");
        }
        xml.push_str("  </engines>\n</component>\n");
        xml
    }
}

/// One `<tag>value</tag>` line per non-empty field. The values are this
/// crate's constants, none of which needs XML escaping.
fn push_elements(xml: &mut String, indent: &str, fields: &[(&str, &str)]) {
    for (tag, value) in fields.iter().filter(|(_, v)| !v.is_empty()) {
        xml.push_str(&format!("{indent}<{tag}>{value}</{tag}>\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::wire::serialized::Context;
    use zbus::wire::{LE, to_bytes};

    fn signature<T: Type>() -> String {
        T::SIGNATURE.to_string()
    }

    // The signatures libibus builds in each `*_serialize`.
    #[test]
    fn signatures_match_libibus() {
        assert_eq!(signature::<Attribute>(), "(sa{sv}uuuu)");
        assert_eq!(signature::<AttrList>(), "(sa{sv}av)");
        assert_eq!(signature::<Text>(), "(sa{sv}sv)");
        assert_eq!(signature::<LookupTable>(), "(sa{sv}uubbiavav)");
        assert_eq!(signature::<EngineDesc>(), "(sa{sv}ssssssssussssssss)");
        assert_eq!(signature::<Component>(), "(sa{sv}ssssssssavav)");
        assert_eq!(signature::<V<Text>>(), "v");
    }

    /// Encode `value` as a variant and decode it back as a generic one,
    /// the way a receiver sees it.
    fn round_trip<T: Type + Serialize>(value: T) -> Variant<'static> {
        let ctxt = Context::new(LE, 0);
        let bytes = to_bytes(ctxt, &V(value)).unwrap();
        let (variant, _): (Variant<'_>, _) = bytes.deserialize().unwrap();
        variant.try_to_owned().unwrap().into()
    }

    #[test]
    fn text_nests_attributes_as_variants() {
        let text = Text::with_attributes(
            "きょう",
            vec![Attribute::new(
                ATTR_TYPE_UNDERLINE,
                ATTR_UNDERLINE_SINGLE,
                0,
                3,
            )],
        );
        let variant = round_trip(text);
        let Variant::Structure(s) = &variant else {
            panic!("not a struct: {variant:?}")
        };
        let fields = s.fields();
        assert_eq!(fields[0], Variant::from("IBusText"));
        assert_eq!(fields[2], Variant::from("きょう"));
        // attrs: v → (s a{sv} av), each attribute a variant (s a{sv} u u u u)
        let Variant::Variant(attrs) = &fields[3] else {
            panic!("attrs not a variant: {:?}", fields[3])
        };
        let Variant::Structure(list) = attrs.as_ref() else {
            panic!("attr list not a struct")
        };
        assert_eq!(list.fields()[0], Variant::from("IBusAttrList"));
        let Variant::Array(items) = &list.fields()[2] else {
            panic!("attributes not an array")
        };
        let Variant::Variant(attr) = &items.inner()[0] else {
            panic!("attribute not a variant")
        };
        let Variant::Structure(attr) = attr.as_ref() else {
            panic!("attribute not a struct")
        };
        let numbers: Vec<_> = attr.fields()[2..].to_vec();
        assert_eq!(
            numbers,
            vec![
                Variant::U32(1),
                Variant::U32(1),
                Variant::U32(0),
                Variant::U32(3)
            ]
        );
    }

    #[test]
    fn surrounding_text_reads_back() {
        assert_eq!(
            text_of(&round_trip(Text::new("今日は"))).as_deref(),
            Some("今日は")
        );
        assert_eq!(text_of(&Variant::U32(1)), None);
    }

    #[test]
    fn lookup_table_labels_each_candidate() {
        let table = LookupTable::new(9, vec![Text::new("今日"), Text::new("京")], 1);
        assert_eq!(table.candidates().collect::<Vec<_>>(), ["今日", "京"]);
        assert_eq!(table.labels().collect::<Vec<_>>(), ["1", "2"]);
        assert_eq!(table.cursor_pos(), 1);
        let Variant::Structure(s) = round_trip(table) else {
            panic!("not a struct")
        };
        assert_eq!(s.fields()[0], Variant::from("IBusLookupTable"));
        assert_eq!(s.fields()[2], Variant::U32(9));
        assert_eq!(s.fields()[6], Variant::I32(ORIENTATION_VERTICAL));
    }

    // The shipped component file is generated from `component()`; this
    // keeps the two from drifting apart.
    #[test]
    fn shipped_xml_matches_component() {
        let shipped = include_str!("../karukan.xml");
        let expected = component(INSTALLED_EXEC.to_string()).to_xml();
        assert_eq!(shipped, expected, "regenerate with `karukan-ibus --xml`");
    }
}
