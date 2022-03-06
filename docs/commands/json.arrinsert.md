---
title: "JSON.ARRINSERT command"
linkTitle: "JSON.ARRINSERT"
type: docs
weight: 1
description: >
    "detailed description"
---

Inserts the `json` values into the array at `path` before the `index` (shifts to the right).

The index must be in the array's range. Inserting at `index` 0 prepends to the array. Negative index values start from the end of the array.

@return

@array-reply of @integer-reply - for each path, the array's new size, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.SET doc $ '{"a":[3], "nested": {"a": [3,4]}}'
OK
redis> JSON.ARRINSERT doc $..a 0 1 2
1) (integer) 3
2) (integer) 4
redis> JSON.GET doc $
"[{\"a\":[1,2,3],\"nested\":{\"a\":[1,2,3,4]}}]"
```

```
redis> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
redis> JSON.ARRINSERT doc $..a 0 1 2
1) (integer) 6
2) (nil)
```

