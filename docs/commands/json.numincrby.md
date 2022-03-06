---
title: "JSON.NUMINCRBY command"
linkTitle: "JSON.NUMINCRBY"
type: docs
weight: 1
description: >
    "detailed description"
---

Increments the number value stored at `path` by `number`.

@return

@bulk-string-reply: the stringified new value for each path, or @null-reply if the matching JSON value is not a number.

@examples

```
redis> JSON.SET doc . '{"a":"b","b":[{"a":2}, {"a":5}, {"a":"c"}]}'
OK
redis> JSON.NUMINCRBY doc $.a 2
"[null]"
redis> JSON.NUMINCRBY doc $..a 2
"[null,4,7,null]"
```
