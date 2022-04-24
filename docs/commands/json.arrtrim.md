Trims an array so that it contains only the specified inclusive range of elements.

This command is extremely forgiving and using it with out-of-range indexes will not produce an error. There are a few differences between how RedisJSON v2.0 and legacy versions handle out-of-range indexes.

Behavior as of RedisJSON v2.0:

* If `start` is larger than the array's size or `start` > `stop`, returns 0 and an empty array. 
* If `start` is < 0, then start from the end of the array.
* If `stop` is larger than the end of the array, it will be treated like the last element.

@return

@array-reply of @integer-reply - for each path, the array's new size, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.ARRTRIM doc $..a 1 1
1) (integer) 0
2) (integer) 1
redis> JSON.GET doc $
"[{\"a\":[],\"nested\":{\"a\":[4]}}]"
```

```sql
redis> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
redis> JSON.ARRTRIM doc $..a 1 1
1) (integer) 1
2) (nil)
```
