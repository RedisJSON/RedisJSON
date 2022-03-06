Reports the number of keys in the JSON Object at `path` in `key`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

@return

@array-reply of @integer-reply - for each path, the number of keys in the object, or @null-reply if the matching JSON value is not an object.

@examples

```
redis> JSON.SET doc $ '{"a":[3], "nested": {"a": {"b":2, "c": 1}}}'
OK
redis> JSON.OBJLEN doc $..a
1) (nil)
2) (integer) 2
```

