import json
import redis

class ReJSON(object):
    """A simple wrapper for ReJSON"""
    def __init__(self, client):
        # Be strict about the Redis client
        if not isinstance(client, redis.client.StrictRedis):
            raise Exception('Invalid Redis client')

        # Ensure that ReJSON is loaded
        loaded = False
        try:
            modules = client.execute_command('MODULE', 'LIST')
        except redis.exceptions.ResponseError:
            raise Exception('\'MODULE LIST\' command erred - you need to use Redis v4 or above')

        for m in modules:
            if m[1] == 'ReJSON':
                loaded = True
                break
        if not loaded:
            raise Exception('ReJSON module not loaded in Redis')

        self._redis = client

    def delete(self, name, path='.'):
        """
        Deletes a value from key ``name`` at ``path``
        """
        return self._redis.execute_command('JSON.DEL', name, path)

    def get(self, name, path='.', *paths):
        """
        Gets the value from the ReJSON key ``name``.

        Additional arguments are paths in the value. If none are given, root is
        returned.
        """
        ser = self._redis.execute_command('JSON.GET', name, path, *paths)
        return json.loads(ser)

    def mget(self, *keys, path):
        """
        Gets a value from a ``path`` in multiple ``keys``

        Returns a dictionary of found ``keys`` and their respective path values.
        """
        resp = self._redis.execute_command('JSON.MGET', *keys, path)
        rep = dict()
        for i, v in enumerate(resp):
            if v is not None:
                rep[keys[i]] = json.loads(v)

        return rep

    def set(self, name, path, value, nx=False, xx=False):
        """
        Set the ReJSON key ``name`` to ``value`` at the ``path``

        ``nx`` if set to True, set the value at key ``name`` to ``value`` if it
          does not already exists.

        ``xx`` if set to True, set the value at key ``name`` to ``value`` if it
          already exists.
        """
        params = [name, path, json.dumps(value)]
        if nx:
            params += 'NX'
        if xx:
            params += 'XX'

        return self._redis.execute_command('JSON.SET', *params)

    def type(self, name, path='.'):
        """
        Gets the type of a value from key ``name`` at ``path``
        """
        return self._redis.execute_command('JSON.TYPE', name, path)

    def exists(self, name, path='.'):
        """
        Checks if the value in ``key`` at ``path`` exists
        """
        return None is not self._redis.execute_command('JSON.TYPE', name, path)

    def numincrby(self, name, path, incrby):
        """
        Increments the the value in ``key`` at ``path`` by ``incrby``
        """
        return self._redis.execute_command('JSON.NUMINCRBY', name, path, json.dumps(incrby))

    def nummultby(self, name, path, multby):
        """
        Multiplies the the value in ``key`` at ``path`` by ``multby``
        """
        return self._redis.execute_command('JSON.NUMMULTBY', name, path, json.dumps(multby))

    def strappend(self, name, path, str):
        """
        Appends ``str`` to string in ``key`` at ``path``
        """
        return self._redis.execute_command('JSON.STRAPPEND', name, path, json.dumps(str))

    def strlen(self, name, path='.'):
        """
        Returns the length of the string in ``key`` at ``path``
        """
        return self._redis.execute_command('JSON.STRLEN', name, path='.')

    def arrappend(self, name, path, *values):
        """
        Appends ``vals`` at the end of the array in ``key`` at ``path``
        """
        svals = [json.dumps(v) for v in values]
        return self._redis.execute_command('JSON.ARRAPPEND', name, path, *svals)

    def arrindex(self, name, path, value, index=0):
        """
        Returns the first occurance of ``value`` in the array in ``key`` at ``path`` starting
        ``index``
        """
        return self._redis.execute_command('JSON.ARRINDEX', name, path, json.dumps(value), index)

    def arrinsert(self, name, path, index, *values):
        """
        Inserts ``values`` in the array in ``key`` at ``path`` starting ``index`` (right-shift)
        """
        svals = [json.dumps(v) for v in values]
        return self._redis.execute_command('JSON.ARRINSERT', name, path, index, *svals)

    def arrlen(self, name, path='.'):
        """
        Returns the length of the array in ``key`` at ``path``
        """
        return self._redis.execute_command('JSON.ARRLEN', name, path)

    def arrpop(self, name, path='.', index=-1):
        """
        Deletes and returns an element from the array in ``key`` at ``path`` at ``index``
        """
        return json.loads(self._redis.execute_command('JSON.ARRPOP', name, path, index))

    def arrtrim(self, name, path, start, stop):
        """
        Trim the array in ``key`` at ``path`` so it contains only the range between ``start`` and
        ``stop``
        """
        return self._redis.execute_command('JSON.ARRTRIM', name, path, start, stop)

    def objkeys(self, name, path='.'):
        """
        Returns the names of keys in the object in ``key`` at ``path``
        """
        return self._redis.execute_command('JSON.OBJKEYS', name, path)

    def objlen(self, name, path='.'):
        """
        Returns the length of the object in ``key`` at ``path``
        """
        return self._redis.execute_command('JSON.OBJLEN', name, path)

    def forget(self, name, path='.'):
        """
        An alias for JSON.del
        """
        return self.delete(name, path='.')

    def resp(self, name, path='.'):
        """
        Returns the RESP form of the value in ``key`` at ``path``
        """
        return self._redis.execute_command('JSON.RESP', name, path)
