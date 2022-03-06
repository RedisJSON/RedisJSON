---
title: "JSON.TYPE command"
linkTitle: "JSON.TYPE"
type: docs
weight: 1
description: >
    "detailed description"
---

Reports the type of JSON value at `path`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

@return

@array-reply of @string-reply - for each path, the value's type.

@examples

```
redis> JSON.SET doc $ '{"a":2, "nested": {"a": true}, "foo": "bar"}'
OK
redis> JSON.TYPE doc $..foo
1) "string"
redis> JSON.TYPE doc $..a
1) "integer"
2) "boolean"
redis> JSON.TYPE doc $..dummy
(empty array)
```

