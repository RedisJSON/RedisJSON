# JSON.PATH

`JSON.PATH` uses the same argument parser, path grouping, and reply formatting as
`JSON.GET`. Each selected value is replaced by its concrete absolute path.
Unlike GET, PATH rejects legacy paths (such as `.name`, `name`, or `$a`).
An omitted path defaults to `$`; GET retains its legacy default and behavior.

```text
JSON.PATH key [INDENT indent] [NEWLINE newline] [SPACE space]
              [FORMAT {STRING|EXPAND1|EXPAND}] [path ...]
```

For `person = {"children":[{"name":"Bob","age":7},{"name":"Emma"}]}`:

```text
JSON.GET person '$.children[*].name'
→ '["Bob","Emma"]'
JSON.PATH person '$.children[*].name'
→ '["$[\"children\"][0][\"name\"]","$[\"children\"][1][\"name\"]"]'
```

Both replies are bulk strings containing JSON. Decode both and pair their array
entries by position. For readability, the following table shows decoded JSON:

| Input expressions | Decoded PATH reply |
|---|---|
| Omitted | `["$"]` |
| `$` | `["$"]` |
| `$.children[*].age` | `["$[\"children\"][0][\"age\"]"]` |
| `.children[*].name` | Error: legacy paths are unsupported |
| `$.missing` | `[]` |
| `.missing` | Error: legacy paths are unsupported |
| `$.children[0].age + 1` | `[null]` (computed value has no stored location) |

Multiple expressions use GET's existing grouping: STRING replies are objects
keyed by the original expressions, each containing a match array. Repeated input expressions collapse into one field as in GET. Duplicate
matches within a query remain duplicated, in query order. Invalid expressions
in multi-input STRING requests retain GET's existing handling.

Missing keys return a Redis null reply. Missing fields are omitted; matched JSON
null values retain their concrete paths. Root/object/array selections each return
one location per selected node, not paths for every descendant.

RESP3 `FORMAT EXPAND` returns native arrays, one per input expression, following GET's
existing JSONPath behavior. `FORMAT EXPAND1` uses GET's
string serialization rules: path strings are themselves JSON-encoded. Formatting
options and RESP2 restrictions are inherited from GET.

## Implementation

Both entry points invoke `json_read_command_impl<M, const PATHS: bool>`: GET uses
`false`, PATH uses `true`. The compile-time mode is passed through the existing
reply builders; it is not a per-match runtime mode check. GET retains its existing
untracked query evaluation and value serializers. PATH uses the existing tracked
visitor, builds path strings directly into preallocated buffers, and retains only
final strings. No JSON document copy or intermediate match vector is introduced
for ordinary PATH queries. Computed projections are evaluated to preserve result
cardinality and then represented by null placeholders.

Result memory is proportional to returned path bytes and entries, plus the query
engine's traversal/filter memory and Redis output buffers. PATH is not free:
tracking, building, and sending paths costs more than many value-only queries.

Use `MULTI/EXEC` around PATH and GET with identical expressions to obtain a
consistent view. The two commands still evaluate the query separately. Decode
STRING replies and pair within each matching group; do not zip map iteration
orders. For EXPAND replies, pair selected nodes, not every nested value within a
selected object.

Member names use JSON escaping. Quotes, backslashes, Unicode, dots, and numeric
member names are supported. The existing parser cannot read back every escaped
control character (for example `\n`); such output identifies a location but cannot
always be reused verbatim as a query. Parser changes are outside this change.
