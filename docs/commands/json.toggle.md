Toggle a boolean value stored at `path`.

return

@integer-reply: specifically the new value (0-false or 1-true), or @null-reply element for JSON values matching the path which are not boolean.

@examples

```
redis> JSON.SET doc $ '{"bool": true}'
OK
redis> JSON.TOGGLE doc $.bool
1) (integer) 0
redis> JSON.GET doc $
"[{\"bool\":false}]"
redis> JSON.TOGGLE doc $.bool
1) (integer) 1
redis> JSON.GET doc $
"[{\"bool\":true}]"
```