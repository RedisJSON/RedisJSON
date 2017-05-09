all:
	$(MAKE) -C ./src all

test:
	$(MAKE) -C ./test all
.PHONY: test

clean:
	find ./ -name "*.[oa]" -exec rm {} \; -print
	find ./ -name "*.so" -exec rm {} \; -print
	find ./ -name "*.out" -exec rm {} \; -print

