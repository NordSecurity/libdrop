
FEATURES ?= --features moose_file
LIB ?= ../target/debug/libnorddrop.so
LIB_DOCKER=${LIB}
COVERAGE_PATH ?= coverage
HTML_PATH ?= html
RUSTFLAGS_COVERAGE ?= -C instrument-coverage -Zprofile -Copt-level=0 -Clink-dead-code -Coverflow-checks=off

.DEFAULT_GOAL := test
.PHONY: build build-coverage test coverage clean

# Build the project
build:
	cargo build $(FEATURES)

# Build the project with coverage instrumentation
build-coverage:
	CARGO_INCREMENTAL=0 RUSTFLAGS="$(RUSTFLAGS_COVERAGE)" cargo build $(FEATURES)

# Test the project
test: build
	LIB_PATH=$(LIB_DOCKER) ./runner.py

# Generate and view coverage report
coverage: build-coverage
	mkdir -p ${COVERAGE_PATH}

	# allow coverage of failed test cases
	LIB_PATH=${LIB_DOCKER} ./runner.py || true

	llvm-profdata merge -sparse $(COVERAGE_PATH)/*.profraw -o full.profdata
	llvm-cov report -instr-profile=full.profdata --ignore-filename-regex='/.cargo/registry' --Xdemangler=rustfilt $(LIB)
	grcov $(COVERAGE_PATH) --binary-path $(LIB) -s .. -t ${HTML_PATH}
	python -m http.server 8080 --directory ./html

# Clean up the project
clean:
	cargo clean
	git clean -fd
