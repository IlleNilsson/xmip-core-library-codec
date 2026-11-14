use super::*;

const ORDER: &str = r#"<?xml version="1.0"?>
<!-- an order -->
<o:order xmlns:o="urn:example:order" xmlns="urn:example:default" currency="SEK"
         o:kind='rush'>
  <o:id>A&amp;1</o:id><line n="1"><sku>X</sku></line>
  <line n="2"><![CDATA[<raw>]]> and more<?note keep?></line>
  <empty xmlns=""/>
</o:order>"#;

#[test]
fn elements_are_read_with_their_namespaces_attributes_and_text() {
    let document = Document::parse(ORDER).expect("reads");
    let root = document.root_element();
    assert_eq!(
        (root.name(), root.prefix(), root.local()),
        ("o:order", "o", "order")
    );
    assert_eq!(root.namespace(), Some("urn:example:order"));
    assert!(root.is(Some("urn:example:order"), "order"));
    assert_eq!(root.attribute("currency"), Some("SEK"));
    assert_eq!(root.attribute("kind"), None);
    assert_eq!(root.attribute_in("urn:example:order", "kind"), Some("rush"));
    let declared: Vec<(&str, &str)> = root.declarations().collect();
    assert_eq!(
        declared,
        [("o", "urn:example:order"), ("", "urn:example:default")]
    );
    let children: Vec<&str> = root.elements().map(Node::local).collect();
    assert_eq!(children, ["id", "line", "line", "empty"]);
    let id = root.elements().next().expect("id");
    assert_eq!(id.text(), Some("A&1"));
    let line = root.elements().nth(1).expect("line");
    assert_eq!(line.namespace(), Some("urn:example:default"));
    let empty = root.elements().nth(3).expect("empty");
    assert_eq!(empty.namespace(), None);
    assert!(empty.closed_itself());
}

#[test]
fn text_and_cdata_side_by_side_are_one_text_and_a_string_value_is_every_text() {
    let document = Document::parse(ORDER).expect("reads");
    let second = document.root_element().elements().nth(2).expect("line");
    let children: Vec<Node<'_>> = second.children().collect();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0].text(), Some("<raw> and more"));
    assert!(children[1].is_instruction());
    assert_eq!(
        (children[1].name(), children[1].text()),
        ("note", Some("keep"))
    );
    assert_eq!(
        document
            .root_element()
            .elements()
            .nth(1)
            .expect("line")
            .string_value(),
        "X"
    );
    let all = document.root_element().string_value();
    assert!(
        all.contains("A&1") && all.contains("<raw> and more"),
        "{all}"
    );
}

#[test]
fn every_node_knows_where_it_lies_and_ids_run_in_document_order() {
    let document = Document::parse(ORDER).expect("reads");
    let root = document.root_element();
    let id = root.elements().next().expect("id");
    assert_eq!(&ORDER[id.range()], "<o:id>A&amp;1</o:id>");
    assert_eq!(&ORDER[id.open_end()..id.close_start()], "A&amp;1");
    let currency = root
        .attributes()
        .find(|a| a.local() == "currency")
        .expect("currency");
    assert_eq!(&ORDER[currency.range()], "SEK");
    assert_eq!(currency.owner(), root);
    let ids: Vec<usize> = root.descendants().map(Node::id).collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(document.root().children().any(Node::is_comment));
    assert_eq!(id.parent(), Some(root));
    assert_eq!(document.root().parent(), None);
}

#[test]
fn line_ends_and_an_attributes_white_space_are_read_as_xml_reads_them() {
    let document = Document::parse("<a b=\"x\r\n\ty\">one\r\ntwo\rthree</a>").expect("reads");
    let root = document.root_element();
    assert_eq!(root.attribute("b"), Some("x  y"));
    assert_eq!(root.text(), Some("one\ntwo\nthree"));
}

#[test]
fn an_owned_document_reads_the_same() {
    let document = Document::parse_owned(ORDER.to_string(), &Options::default()).expect("reads");
    assert_eq!(document.root_element().local(), "order");
    assert_eq!(document.source(), ORDER);
}

#[test]
fn what_the_estate_does_not_read_is_refused_where_it_stands() {
    let refused = |text: &str| Document::parse(text).expect_err("refused");
    let doctype = refused("<!DOCTYPE a [<!ENTITY x \"y\">]><a>&x;</a>");
    assert!(doctype.message.contains("document type"), "{doctype}");
    assert_eq!(doctype.at, 0);
    let entity = refused("<a>&nbsp;</a>");
    assert!(entity.message.contains("&nbsp;"), "{entity}");
    assert_eq!(entity.at, 3);
    let unbound = refused("<a><p:b/></a>");
    assert_eq!(unbound.message, "the prefix 'p' is bound to no namespace");
    assert_eq!(unbound.at, 3);
    let twice = refused("<a xmlns:p='u' xmlns:q='u' p:x='1' q:x='2'/>");
    assert!(twice.message.contains("given twice"), "{twice}");
    let again = refused("<a x='1' x='2'/>");
    assert_eq!(again.message, "the attribute 'x' is given twice");
    let crossed = refused("<a><b></a>");
    assert_eq!(
        (crossed.message.as_str(), crossed.at),
        ("the end tag does not close the open element", 6)
    );
    let unbinding = refused("<a xmlns:p=''/>");
    assert!(unbinding.message.contains("no namespace"), "{unbinding}");
}

#[test]
fn options_refuse_cdata_instructions_and_depth_past_what_they_allow() {
    let strict = Options {
        depth: 2,
        cdata: false,
        instructions: false,
    };
    let refused = |text: &str| Document::parse_with(text, &strict).expect_err("refused");
    assert!(refused("<a><![CDATA[x]]></a>").message.contains("CDATA"));
    assert!(
        refused("<a><?pi x?></a>")
            .message
            .contains("processing instruction")
    );
    assert!(
        refused("<a><b><c/></b></a>")
            .message
            .contains("nests deeper than 2")
    );
    assert!(Document::parse_with("<?xml version='1.0'?><a><b/></a>", &strict).is_ok());
    let deep = format!("{}{}", "<a>".repeat(65), "</a>".repeat(65));
    assert!(Document::parse(&deep).is_err());
}
