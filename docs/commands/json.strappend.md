Appends the `json-string` values to the string at `path`.

`path` defaults to root if not provided.

@return

@array-reply of @integer-reply - for each path, the string's new length, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.SET doc $ '{"a":"foo", "nested": {"a": "hello"}, "nested2": {"a": 31}}'
OK
redis> JSON.STRAPPEND doc $..a '"baz"'
1) (integer) 6
2) (integer) 8
3) (nil)
redis> JSON.GET doc $
"[{\"a\":\"foobaz\",\"nested\":{\"a\":\"hellobaz\"},\"nested2\":{\"a\":31}}]"
```
