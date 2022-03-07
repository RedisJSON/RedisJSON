Reports the length of the JSON Array at `path` in `key`.

`path` defaults to root if not provided. Returns null if the `key` or `path` do not exist.

@return

@array-reply of @integer-reply - for each path, the array's length, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.SET doc $ '{"a":[3], "nested": {"a": [3,4]}}'
OK
redis> JSON.ARRLEN doc $..a
1) (integer) 1
2) (integer) 2
```

```
redis> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
redis> JSON.ARRLEN doc $..a
1) (integer) 4
2) (nil)
```
