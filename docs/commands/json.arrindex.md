Searches for the first occurrence of a scalar JSON value in an array.

The optional inclusive `start` (default 0) and exclusive `stop` (default 0, meaning that the last element is included) specify a slice of the array to search.
Negative values are interpreted as starting from the end.


Note: out-of-range indexes round to the array's start and end. An inverse index range (such as the range from 1 to 0) will return unfound.

@return

@array-reply of @integer-reply - the first position in the array of each JSON value that matches the path, -1 if unfound in the array, or @nil-reply if the matching JSON value is not an array.

@examples

```
redis> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": [3,4]}}'
OK
redis> JSON.ARRINDEX doc $..a 2
1) (integer) 1
2) (integer) -1
```

```
redis> JSON.SET doc $ '{"a":[1,2,3,2], "nested": {"a": false}}'
OK
redis> JSON.ARRINDEX doc $..a 2
1) (integer) 1
2) (nil)
```
