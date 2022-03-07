Removes and returns an element from the index in the array.

`path` defaults to root if not provided. `index` is the position in the array to start popping from (defaults to -1, meaning the last element). Out-of-range indexes round to their respective array ends. Popping an empty array returns null.

@return

@array-reply of @bulk-string-reply - for each path, the popped JSON value, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.SET doc $ '{"a":[3], "nested": {"a": [3,4]}}'
OK
redis> JSON.ARRPOP doc $..a
1) "3"
2) "4"
redis> JSON.GET doc $
"[{\"a\":[],\"nested\":{\"a\":[3]}}]"
```

```
redis> JSON.SET doc $ '{"a":["foo", "bar"], "nested": {"a": false}, "nested2": {"a":[]}}'
OK
redis> JSON.ARRPOP doc $..a
1) "\"bar\""
2) (nil)
3) (nil)
```
