Returns the keys in the object that's referenced by `path`.

`path` defaults to root if not provided. Returns null if the object is empty or either `key` or `path` do not exist.

@return

@array-reply of @array-reply - for each path, an array of the key names in the object as @bulk-string-reply, or @nil-reply if the matching JSON value is not an object. 

@examples

```
redis> JSON.SET doc $ '{"a":[3], "nested": {"a": {"b":2, "c": 1}}}'
OK
redis> JSON.OBJKEYS doc $..a
1) (nil)
2) 1) "b"
   2) "c"
```
