
FEATURES ?= --features moose_file
LIB ?= ../target/debug/libnorddrop.so
LIB_DOCKER=${LIB}
COVERAGE_PATH ?= coverage
HTML_PATH ?= html
RUSTFLAGS_COVERAGE ?= -C instrument-coverage -Zprofile -Copt-level=0 -Clink-dead-code -Coverflow-checks=off

SCENARIOS_AT_ONCE ?= 10
TESTCASE_TIMEOUT ?= 100

.DEFAULT_GOAL := test
.PHONY: build build-coverage test coverage clean docker-prepare build-image

$(LIB): $(shell find ../ -type f \( -name '*.rs' -o -name '*.toml' -o -name '*.sql' -o -name '*.lock' \))
	cargo build $(FEATURES)
	
# Build the project with coverage instrumentation
build-coverage:
	CARGO_INCREMENTAL=0 RUSTFLAGS="$(RUSTFLAGS_COVERAGE)" cargo +nightly build $(FEATURES)

build-image:
	docker build -t libdroptestimage .

docker-prepare:
	-docker kill $(docker ps -q)
	docker network prune -f
	docker container prune -f

# Test the project
test: $(LIB) build-image docker-prepare
	LIB_PATH=$(LIB_DOCKER) ./runner.py --testcase-timeout=$(TESTCASE_TIMEOUT) --scenarios-at-once=$(SCENARIOS_AT_ONCE)

# Generate and view coverage report
coverage: docker-prepare build-coverage
	rm -rf ${COVERAGE_PATH}
	mkdir -p ${COVERAGE_PATH}

	# allow coverage of failed test cases
	-LIB_PATH=${LIB_DOCKER} ./runner.py

	llvm-profdata merge -sparse $(COVERAGE_PATH)/*.profraw -o full.profdata
	llvm-cov report -instr-profile=full.profdata --ignore-filename-regex='/.cargo/registry' --Xdemangler=rustfilt $(LIB)
	grcov $(COVERAGE_PATH) --binary-path $(LIB_DOCKER) -s .. -t ${HTML_PATH}
	python -m http.server 8080 --directory ./html

# Clean up the project
clean:
	cargo clean
	git clean -fd
