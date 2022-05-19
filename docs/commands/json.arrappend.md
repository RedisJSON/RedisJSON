Append the `json` values into the array at `path` after the last element in it.

@return

@array-reply of @integer-reply - for each path, the array's new size, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.SET doc $ '{"a":[1], "nested": {"a": [1,2]}, "nested2": {"a": 42}}'
OK
redis> JSON.ARRAPPEND doc $..a 3 4
1) (integer) 3
2) (integer) 4
3) (nil)
redis> JSON.GET doc $
"[{\"a\":[1,3,4],\"nested\":{\"a\":[1,2,3,4]},\"nested2\":{\"a\":42}}]"
```
