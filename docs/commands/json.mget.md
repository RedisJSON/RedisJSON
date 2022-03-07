Returns the values at `path` from multiple `key` arguments. Returns null for nonexistent keys and nonexistent paths.

@return

@array-reply of @bulk-string-reply - the JSON serialization of the value at each key's
path.

@examples

Given the following documents:

```
redis> JSON.SET doc1 $ '{"a":1, "b": 2, "nested": {"a": 3}, "c": null}'
OK
redis> JSON.SET doc2 $ '{"a":4, "b": 5, "nested": {"a": 6}, "c": null}'
OK
```

```
redis> JSON.MGET doc1 doc2 $..a
1) "[1,3]"
2) "[4,6]"
```
