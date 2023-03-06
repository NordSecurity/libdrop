ARCH ?= x86_64
LIB_PATH ?= dist/linux/release/$(ARCH)/libnorddrop.so

build:
	../../ci/build.py build linux $(ARCH)

run:
	LIB_PATH=$(LIB_PATH) ./runner.py
