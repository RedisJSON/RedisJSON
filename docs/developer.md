# Developer notes

## Testing

Python is required for ReJSON's module test. Install it with `apt-get install python`. You'll also
need to have [redis-py](https://github.com/andymccurdy/redis-py) installed. The easiest way to get
it is using pip and running `pip install redis`.

The module's test can be run against an "embedded" disposable Redis instance, or against an instance
you provide to it. The "embedded" mode requires having the `redis-server` executable in your `PATH`.
To run the tests, run the following in the project's directory:

```bash
$ # use a disposable Redis instance for testing the module
$ make test
```

You can override the spawning of the embedded server by specifying a Redis port via the `REDIS_PORT`
environment variable, e.g.:

```bash
$ # use an existing local Redis instance for testing the module
$ REDIS_PORT=6379 make test
```

## Making the docs

1. You'll need `mkdocs`, install it with: `pip install mkdocs`
1. You'll also need the theme so: `pip install mkdocs-material`
1. To serve locally do: `mkdocs build && mkdocs serve`
1. To upload to GitHub Pages do: `mkdocs gh-deploy`