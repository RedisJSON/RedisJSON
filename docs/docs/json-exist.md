# JSON.EXIST

`JSON.EXIST key path` returns integer `1` if the path selects at least one
stored JSON value, or `0` if the key is missing or the path has no matches.
JSON null, false, zero, empty strings, arrays and objects all count as existing.

The command uses the existing path parser, including legacy paths. Invalid
expressions, computed projections and non-JSON keys return errors. The path is
required; multiple paths and formatting options are not accepted.

```text
JSON.SET doc $ '{"a":null}'
JSON.EXIST doc $.a       => 1
JSON.EXIST doc $.missing => 0
```

Traversal stops at the first match without collecting values or constructing
path strings. This read-only command does not create keys or paths. A separate
existence check does not make a later write atomic.
