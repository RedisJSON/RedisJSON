Clears container values (Arrays/Objects), sets numeric values to `0`, sets string value to emptys, and sets boolean values to `false`.

Already cleared values are ignored: empty containers, zero numbers, empty strings, `false`, and `null`.

`path` defaults to root if not provided. Non-existing paths are ignored.

@return

@integer-reply: specifically the number of values cleared.

@examples

```
redis> JSON.SET doc $ '{"obj":{"a":1, "b":2}, "arr":[1,2,3], "str": "foo", "bool": true, "int": 42, "float": 3.14}'
OK
redis> JSON.CLEAR doc $.*
(integer) 6
redis> JSON.GET doc $
"[{\"obj\":{},\"arr\":[],\"str\":\"\",\"bool\":false,\"int\":0,\"float\":0}]"
```
