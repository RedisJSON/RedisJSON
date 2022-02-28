Reports the length of the JSON String at `path` in `key`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

@return

@array-reply of @integer-reply - for each path, the string's length, or @null-reply if the matching JSON value is not a string.


@examples

```
redis> JSON.SET doc $ '{"a":"foo", "nested": {"a": "hello"}, "nested2": {"a": 31}}'
OK
redis> JSON.STRLEN doc $..a
1) (integer) 3
2) (integer) 5
3) (nil)
```
