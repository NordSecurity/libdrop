OSES := windows linux android darwin
LANGS := go java csharp

SWIG_linux := -D__unix__ -D__linux__
SWIG_android := -D__unix__ -D__ANDROID__
SWIG_darwin := -D__unix__ -D__APPLE__
SWIG_windows := -D_WIN32

FFI_DIR := ffi
BIND_DIR := $(FFI_DIR)/bindings
HELP_DIR := $(FFI_DIR)/helpers
CFG := $(BIND_DIR)/norddrop.i

GO_MODULE := norddropgo

JAVA_PKG := com.nordsec.norddrop
JAVA_PATH := $(subst .,/,$(JAVA_PKG))

CS_NS := NordSec.NordDrop

PATH := $(PATH):$(HELP_DIR)

all: bindings

clean:
	rm -rf $(BIND_DIR)

headers:
	cbindgen -c cbindgen.toml -o $(FFI_DIR)/norddrop.h
	cbindgen -c cbindgen.toml -o $(FFI_DIR)/norddrop_types.h src/ffi/types.rs

binding_c:
	mkdir -p $(BIND_DIR)
	cbindgen -c cbindgen.toml -o $(BIND_DIR)/norddrop.h
	cbindgen -c cbindgen.toml -o $(BIND_DIR)/norddrop_types.h src/ffi/types.rs
	
	cp $(FFI_DIR)/*.i $(BIND_DIR)

define define_bindings
$(eval
	OS := $(1)
	SWIG := swig $(SWIG_$(OS))
	WD := $(BIND_DIR)/$(OS)/wrap
	GD := $(BIND_DIR)/$(OS)/go
	SD := $(BIND_DIR)/$(OS)/csharp
	JD := $(BIND_DIR)/$(OS)/java
)

base_$(OS): binding_c
	mkdir -p $(WD)
	cp $(HELP_DIR)/wrap/* $(WD)

binding_go_$(OS): base_$(OS)
	mkdir -p $(GD)
	$(SWIG) -go -module $(GO_MODULE) -intgosize 32 -cgo -outdir $(GD) -o $(WD)/go_wrap.c $(CFG)
	sed -i 's/^\(#include <stdint.h>\)$$$$/\1\n#include "wtf.h"/' $(GD)/norddropgo.go
	sed -i 's/type swig_gostring struct { p uintptr; n int }/type swig_gostring struct { p uintptr; n int32 }/' $(GD)/norddropgo.go
	cp $(HELP_DIR)/go/* $(GD)

binding_csharp_$(OS): base_$(OS)
	mkdir -p $(SD)
	# Generate just c wrapper.
	$(SWIG) -csharp -namespace $(CS_NS) -outdir /dev -outfile null -o $(WD)/csharp_wrap.c $(CFG)
	# Generate just C# wrapper, since cscode works only in -c++ mode
	$(SWIG) -D_WIN32 -c++ -csharp -namespace $(CS_NS) -outdir $(SD) -outfile NordDrop.cs -o /dev/null $(CFG)
	sed -i 's/"libnorddrop"/"norddrop"/' $(SD)/NordDrop.cs

binding_java_$(OS): base_$(OS)
	mkdir -p $(JD)/$(JAVA_PATH)
	$(SWIG) -java -package $(JAVA_PKG) -outdir $(JD)/$(JAVA_PATH) -o $(WD)/java_wrap.c $(CFG)
	cp $(HELP_DIR)/java/* $(JD)/$(JAVA_PATH)
	sed -i 's/%JAVA_PKG%/$(JAVA_PKG)/' $(JD)/$(JAVA_PATH)/I*.java
	rm $(JD)/$(JAVA_PATH)/libnorddrop.java  #remove empty java class from artifacts

binding_$(OS): $(foreach L,$(LANGS),binding_$(L)_$(OS))

endef
$(foreach OS,$(OSES),$(eval $(call define_bindings,$(OS))))

bindings: clean $(foreach OS,$(OSES),binding_$(OS))

.PHONY: clean headers bindings

.EXPORT_ALL_VARIABLES:
