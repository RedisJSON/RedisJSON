Multiplies the number value stored at `path` by `number`.

@return

@bulk-string-reply - the stringified new values for each path, or @nil-reply element if the matching JSON value is not a number.

@examples

```
redis> JSON.SET doc . '{"a":"b","b":[{"a":2}, {"a":5}, {"a":"c"}]}'
OK
redis> JSON.NUMMULTBY doc $.a 2
"[null]"
redis> JSON.NUMMULTBY doc $..a 2
"[null,4,10,null]"
```
