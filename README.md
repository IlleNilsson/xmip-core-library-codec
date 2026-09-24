# xmip-core-library-codec

Encoding primitives every layer of Xmip shares: how a format's characters
and bytes are written and read back, never what they mean. What a format
means stays with the format's own crate.

Until 2026-09-22 each capability wrote these by hand, and copies drifted
apart: the two SAML gates unescaped the same assertion differently, so one
name could be two principals. One copy lives here, and every reader uses it.

| Module | What it is |
| --- | --- |
| `char_reader` | `CharReader`, the character reader every lexer walks its text with: positions always on a character boundary, lines and columns kept, Unicode whitespace skipped whole |
| `toml` | Quoting text as a TOML 1.0 basic string, every control character escaped, and reading one back strictly |
| `xml` | Escaping text and attribute values, and unescaping the five predefined entities and character references |

The lexers of FHIRPath, the predicate language, JSONPath, GraphQL, SQL, the
proto3 language and the SQL archive's statement read through `CharReader`.
Until 2026-09-24 each counted positions itself, and two stepped over
whitespace a byte at a time and panicked inside U+00A0. The node's
declaration and the file archive's sidecar quote through `toml`; the node's
own copy escaped three characters.

`architecture.toml` carries the maturity.
