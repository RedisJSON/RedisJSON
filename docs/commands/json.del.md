Deletes a value.

`path` defaults to root if not provided. Ignores nonexistent keys and paths. Deleting an object's root is equivalent to deleting the key from Redis.

@return

@integer-reply - the number of paths deleted (0 or more).

@examples

```
redis> JSON.SET doc $ '{"a": 1, "nested": {"a": 2, "b": 3}}'
OK
redis> JSON.DEL doc $..a
(integer) 2
redis> JSON.GET doc $
"[{\"nested\":{\"b\":3}}]"
```
