# xmip-core-library-codec

Encoding primitives every layer of Xmip shares: how a format's characters
and bytes are written and read back, never what they mean. What a format
means stays with the format's own crate.

Until 2026-09-22 each capability wrote these by hand, and copies drifted
apart: the two SAML gates unescaped the same assertion differently, so one
name could be two principals. One copy lives here, and every reader uses it.

| Module | What it is |
| --- | --- |
| `xml` | Escaping text and attribute values, and unescaping the five predefined entities and character references |

`architecture.toml` carries the maturity.
