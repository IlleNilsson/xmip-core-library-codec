# xmip-core-library-codec

Encoding primitives every layer of Xmip shares: how a format's characters
and bytes are written and read back, never what they mean. What a format
means stays with the format's own crate.

Until 2026-09-22 each capability wrote these by hand, and copies drifted
apart: the two SAML gates unescaped the same assertion differently, so one
name could be two principals. One copy lives here, and every reader uses it.

| Module | What it is |
| --- | --- |
| `base64` | Base 64 (RFC 4648 section 4) and base64url (section 5), padded or not, and a strict decoder: nothing outside the alphabet, padding only where the length needs it, no bits past the last byte |
| `char_reader` | `CharReader`, the character reader every lexer walks its text with: positions always on a character boundary, lines and columns kept, Unicode whitespace skipped whole |
| `civil` | The civil calendar: `CivilTime`, seconds since the epoch to a UTC date and time of day and back, the weekday, RFC 3339 |
| `crc` | `Crc`, the one CRC, parameterised as the CRC catalogue writes each down, and each CRC a protocol frames with as a named constant held to its catalogue check value: `CRC_16_KERMIT` (IEEE 802.15.4), `CRC_16_DNP`, `CRC_16_EN_13757`, `CRC_32_ISCSI` (CRC-32C), `CRC_8_TS_27_010` (RFCOMM) |
| `cursor` | `Cursor`, the one byte cursor a binary message's fields are read with: bytes, NUL-terminated fields, varints, and integers and floats in either byte order, never past the end |
| `hex` | Base 16: bytes as lower-case hex pairs, and either case back |
| `sha1` | SHA-1 (FIPS 180-4), for the two protocols that still name it: the WebSocket accept key and `MySQL`'s native password |
| `sql` | `Delimiter`: SQL's delimited text — a `'…'` literal, an `"…"`, `[…]` or backtick identifier — written with the closing delimiter doubled and read back with the doubling undone; which delimiter, and anything else a server adds, is the dialect's |
| `toml` | Quoting text as a TOML 1.0 basic string, every control character escaped, and reading one back strictly |
| `varint` | The base-128 varint (unsigned LEB128) and the zig-zag mapping of a signed integer onto it |
| `writer` | `ByteWriter`, the one byte writer, on `Vec<u8>`: the counterpart of `Cursor` |
| `xml` | Escaping text and attribute values, and unescaping the five predefined entities and character references |

The lexers of FHIRPath, the predicate language, JSONPath, GraphQL, SQL, the
proto3 language and the SQL archive's statement read through `CharReader`.
Until 2026-09-24 each counted positions itself, and two stepped over
whitespace a byte at a time and panicked inside U+00A0. The node's
declaration and the file archive's sidecar quote through `toml`; the node's
own copy escaped three characters. The SQL Server, `MySQL` and `PostgreSQL`
transports, the SQL script archive and the SQL contract's lexer quote and
read SQL through `sql`; until 2026-09-24 each wrote the rule itself, and two
far ends read a doubled identifier delimiter as the identifier's end.

The bytes followed on 2026-09-24. Hex had been written twelve times, base 64
three, SHA-1 twice, a CRC five times, a byte cursor fourteen, the varint and
its zig-zag five and the civil calendar seven, one of them inside
`xmip-core-library-asn1`, which now reads a `GeneralizedTime` through `civil`.
The transport capability's own cursor, hex and CRC went with them. Every
binary technology — Kafka, TDS, `PostgreSQL`, `MySQL`, MQTT, AMQP, XDR, OPC UA,
Oracle's TTC, SSH, Matter TLV — reads through `Cursor` and writes through
`ByteWriter`; what a field means (XDR's padding, AMQP's short string, a
length-encoded integer) stays in the protocol's crate, as a trait over them.

`architecture.toml` carries the maturity.
