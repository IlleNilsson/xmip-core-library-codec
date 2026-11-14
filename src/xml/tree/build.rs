//! A document's pieces, as the [`Reader`] gives them, read into a tree:
//! namespaces resolved as each element opens, text read and adjacent runs
//! made one, and what [`Options`] or the estate refuses refused where it
//! stands.

use std::borrow::Cow;

use super::{
    AttributeData, Declaration, Document, ElementData, Kind, Namespace, NodeData, Options, Refused,
    Value,
};
use crate::xml::event::{Event, Span, Tag, offset};
use crate::xml::reader::Reader;
use crate::xml::{attribute_value, entity, line_ends};

/// What the tree is made of, gathered as the pieces come.
#[derive(Default)]
struct Parts {
    nodes: Vec<NodeData>,
    elements: Vec<ElementData>,
    attributes: Vec<AttributeData>,
    declarations: Vec<Declaration>,
    read: Vec<String>,
}

/// A node open to more children.
struct Open {
    node: usize,
    last: Option<usize>,
    /// How many bindings were in scope when it opened.
    scope: usize,
    /// Whether its last child is text, which more text joins.
    text: bool,
}

struct Builder<'s> {
    source: &'s str,
    options: Options,
    parts: Parts,
    open: Vec<Open>,
    /// The bindings in scope: a prefix, empty for the default namespace,
    /// and what it is bound to.
    scope: Vec<(Span, Namespace)>,
    /// A start tag's attributes, held until its namespaces are known.
    pending: Vec<(Span, Span)>,
}

pub(super) fn build<'a>(source: Cow<'a, str>, options: &Options) -> Result<Document<'a>, Refused> {
    let parts = {
        let text: &str = &source;
        let mut builder = Builder::new(text, *options);
        let mut reader = Reader::document(text);
        while let Some(piece) = reader.read()? {
            builder.take(piece.event, piece.span)?;
        }
        let mut parts = builder.parts;
        parts.nodes[0].end = parts.nodes.len();
        parts.nodes[0].span = Span::new(0, text.len());
        parts
    };
    Ok(Document {
        source,
        nodes: parts.nodes,
        elements: parts.elements,
        attributes: parts.attributes,
        declarations: parts.declarations,
        read: parts.read,
    })
}

impl<'s> Builder<'s> {
    fn new(source: &'s str, options: Options) -> Self {
        let mut parts = Parts::default();
        // Sized once from the text, so a large document is never copied.
        parts.nodes.reserve(source.len() / 16);
        parts.elements.reserve(source.len() / 40);
        parts.attributes.reserve(source.len() / 30);
        parts.nodes.push(NodeData {
            kind: Kind::Root,
            parent: 0,
            next: 0,
            end: 1,
            span: Span::default(),
        });
        Self {
            source,
            options,
            parts,
            open: vec![Open {
                node: 0,
                last: None,
                scope: 0,
                text: false,
            }],
            scope: Vec::new(),
            pending: Vec::new(),
        }
    }

    fn span(&self, part: &str) -> Span {
        let start = offset(self.source, part);
        Span::new(start, start + part.len())
    }

    fn take(&mut self, event: Event<'s>, whole: Span) -> Result<(), Refused> {
        match event {
            Event::Text(text) if self.open.len() > 1 => {
                let written = self.span(text);
                self.text(whole, line_ends(text), Some(written));
            }
            Event::Reference(name) => {
                let character =
                    entity(name).map_err(|why| Refused::new(why.message, whole.start))?;
                self.text(whole, Cow::Owned(character.to_string()), None);
            }
            Event::CData(_) if !self.options.cdata => {
                return Err(Refused::new(
                    "the XML holds a CDATA section, which this reader refuses",
                    whole.start,
                ));
            }
            Event::CData(data) => {
                let written = self.span(data);
                self.text(whole, line_ends(data), Some(written));
            }
            Event::Comment(text) => {
                let comment = Kind::Comment(self.span(text));
                self.attach(comment, whole, false);
            }
            // White space around the root, and the XML declaration.
            Event::Text(_) | Event::Instruction { target: "xml", .. } => {}
            Event::Instruction { .. } if !self.options.instructions => {
                return Err(Refused::new(
                    "the XML holds a processing instruction, which this reader refuses",
                    whole.start,
                ));
            }
            Event::Instruction { target, data } => {
                let instruction = Kind::Instruction {
                    target: self.span(target),
                    data: self.span(data),
                };
                self.attach(instruction, whole, false);
            }
            Event::Doctype(_) => {
                return Err(Refused::new(
                    "the XML holds a document type declaration (DTD), which Xmip never reads",
                    whole.start,
                ));
            }
            Event::Start(tag) => self.start(tag, whole)?,
            Event::End(_) => self.end(whole),
        }
        Ok(())
    }

    /// `node` added as the last child of the node open last.
    fn attach(&mut self, kind: Kind, span: Span, text: bool) -> usize {
        let index = self.parts.nodes.len();
        let Some(top) = self.open.last_mut() else {
            return index;
        };
        if let Some(last) = top.last {
            self.parts.nodes[last].next = index;
        }
        top.last = Some(index);
        top.text = text;
        let parent = top.node;
        self.parts.nodes.push(NodeData {
            kind,
            parent,
            next: 0,
            end: index + 1,
            span,
        });
        index
    }

    /// Text read as `read`, written at `written` where it reads as written,
    /// joined to the text before it where there is some.
    fn text(&mut self, whole: Span, read: Cow<'_, str>, written: Option<Span>) {
        let joined = self
            .open
            .last()
            .filter(|top| top.text)
            .and_then(|top| top.last);
        let Some(last) = joined else {
            let value = self.value(read, written);
            self.attach(Kind::Text(value), whole, true);
            return;
        };
        let Kind::Text(before) = self.parts.nodes[last].kind else {
            return;
        };
        let value = match (before, written, &read) {
            (Value::Written(was), Some(now), Cow::Borrowed(_)) if was.end == now.start => {
                Value::Written(Span::new(was.start, now.end))
            }
            (Value::Written(was), _, _) => {
                let mut joined = self.source[was.range()].to_string();
                joined.push_str(&read);
                self.parts.read.push(joined);
                Value::Read(self.parts.read.len() - 1)
            }
            (Value::Read(index), _, _) => {
                self.parts.read[index].push_str(&read);
                Value::Read(index)
            }
        };
        let node = &mut self.parts.nodes[last];
        node.kind = Kind::Text(value);
        node.span.end = whole.end;
    }

    fn value(&mut self, read: Cow<'_, str>, written: Option<Span>) -> Value {
        match (read, written) {
            (Cow::Borrowed(_), Some(span)) => Value::Written(span),
            (read, _) => {
                self.parts.read.push(read.into_owned());
                Value::Read(self.parts.read.len() - 1)
            }
        }
    }

    /// The namespace `prefix` is bound to; the empty prefix is the default
    /// namespace's, which only an element's name is in.
    fn resolve(&self, prefix: &str) -> Option<Namespace> {
        if prefix == "xml" {
            return Some(Namespace::Xml);
        }
        let bound = self
            .scope
            .iter()
            .rev()
            .find(|(declared, _)| &self.source[declared.range()] == prefix);
        match bound {
            Some((_, namespace)) => Some(*namespace),
            None if prefix.is_empty() => Some(Namespace::None),
            None => None,
        }
    }

    fn start(&mut self, tag: Tag<'s>, whole: Span) -> Result<(), Refused> {
        if self.open.len() > self.options.depth {
            return Err(Refused::new(
                format!("the XML nests deeper than {} elements", self.options.depth),
                whole.start,
            ));
        }
        let scope = self.scope.len();
        let declared = self.parts.declarations.len();
        self.pending.clear();
        for written in tag.attributes() {
            let written = written?;
            let value = self.span(written.value);
            match written.name.strip_prefix("xmlns") {
                Some("") => self.declare(Span::new(0, 0), written.value, value)?,
                Some(prefix) if prefix.starts_with(':') => {
                    let prefix = self.span(&prefix[1..]);
                    self.declare(prefix, written.value, value)?;
                }
                _ => {
                    let name = self.span(written.name);
                    self.pending.push((name, value));
                }
            }
        }
        let name = tag.name();
        let (prefix, local) = name
            .split_once(':')
            .map_or(("", 0), |(p, _)| (p, p.len() + 1));
        let namespace = self.resolve(prefix).ok_or_else(|| unbound(prefix, whole))?;
        let index = self.parts.nodes.len();
        let attributes = self.attributes(index, whole)?;
        let element = self.parts.elements.len();
        self.parts.elements.push(ElementData {
            name: self.span(name),
            local,
            namespace,
            attributes,
            declarations: (declared, self.parts.declarations.len()),
            open_end: whole.end,
            close_start: whole.end,
        });
        self.attach(Kind::Element(element), whole, false);
        if tag.empty() {
            self.scope.truncate(scope);
        } else {
            self.open.push(Open {
                node: index,
                last: None,
                scope,
                text: false,
            });
        }
        Ok(())
    }

    fn declare(&mut self, prefix: Span, written: &str, value: Span) -> Result<(), Refused> {
        let uri = attribute_value(written).map_err(|why| Refused::new(why.message, value.start))?;
        if uri.is_empty() && !prefix.is_empty() {
            return Err(Refused::new(
                format!(
                    "the prefix '{}' is declared with no namespace",
                    &self.source[prefix.range()]
                ),
                value.start,
            ));
        }
        let unbinds = uri.is_empty();
        let uri = self.value(uri, Some(value));
        self.parts.declarations.push(Declaration { prefix, uri });
        let namespace = if unbinds {
            Namespace::None
        } else {
            Namespace::Declared(self.parts.declarations.len() - 1)
        };
        self.scope.push((prefix, namespace));
        Ok(())
    }

    /// The pending attributes of the element at `owner`, their namespaces
    /// resolved and their values read; where they lie among all.
    fn attributes(&mut self, owner: usize, whole: Span) -> Result<(usize, usize), Refused> {
        let first = self.parts.attributes.len();
        for index in 0..self.pending.len() {
            let (name, written) = self.pending[index];
            let text = &self.source[name.range()];
            let (prefix, local) = text
                .split_once(':')
                .map_or(("", 0), |(p, _)| (p, p.len() + 1));
            let namespace = if prefix.is_empty() {
                Namespace::None
            } else {
                self.resolve(prefix).ok_or_else(|| unbound(prefix, whole))?
            };
            // The same name twice, or two prefixes for one namespace.
            let twice = self.parts.attributes[first..].iter().any(|other| {
                let named = &self.source[other.name.range()];
                named == text
                    || (namespace != Namespace::None
                        && named[other.local..] == text[local..]
                        && self.same(other.namespace, namespace))
            });
            if twice {
                return Err(Refused::new(
                    format!("the attribute '{text}' is given twice"),
                    whole.start,
                ));
            }
            let read = attribute_value(&self.source[written.range()])
                .map_err(|why| Refused::new(why.message, written.start))?;
            let value = self.value(read, Some(written));
            self.parts.attributes.push(AttributeData {
                name,
                local,
                namespace,
                value,
                written,
                quote: self.source.as_bytes()[written.start - 1],
                owner,
            });
        }
        Ok((first, self.parts.attributes.len()))
    }

    /// Whether two namespaces are one, by what they name.
    fn same(&self, one: Namespace, other: Namespace) -> bool {
        let named = |namespace: Namespace| match namespace {
            Namespace::None => None,
            Namespace::Xml => Some(super::XML_NAMESPACE),
            Namespace::Declared(index) => Some(match self.parts.declarations[index].uri {
                Value::Written(span) => &self.source[span.range()],
                Value::Read(read) => self.parts.read[read].as_str(),
            }),
        };
        named(one) == named(other)
    }

    fn end(&mut self, whole: Span) {
        let Some(open) = self.open.pop() else {
            return;
        };
        let end = self.parts.nodes.len();
        let node = &mut self.parts.nodes[open.node];
        node.span.end = whole.end;
        node.end = end;
        if let Kind::Element(element) = node.kind {
            self.parts.elements[element].close_start = whole.start;
        }
        self.scope.truncate(open.scope);
    }
}

fn unbound(prefix: &str, whole: Span) -> Refused {
    Refused::new(
        format!("the prefix '{prefix}' is bound to no namespace"),
        whole.start,
    )
}
