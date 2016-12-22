all: glib-datatypes

CFLAGS+=$(shell pkg-config glib-2.0 --cflags) -I../
LDFLAGS+=$(shell pkg-config glib-2.0 --libs)

glib-datatypes: glib-datatypes.c
	$(CC) $(CFLAGS) $^ -o $@ $(LDFLAGS)

clean:
	rm -f glib-datatypes
