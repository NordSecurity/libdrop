#if SWIGGO
%go_import("unsafe")

%insert(go_wrapper) %{
var eventCallbacks = map[uintptr]func(string){}
var loggerCallbacks = map[uintptr]func(int, string){}
var pubkeyCallbacks = map[uintptr]func(byte[]) *byte[]{}
// Note: This can only ensure enough place for 8 callbacks
// Application can crash when creating more if these the last
// items on stack
// The real fix for this would be to avoid using pointers where not necessary. In this case - key to hashmap
var arbitraryValue = uint64(0)
var arbitraryAddress = uintptr(unsafe.Pointer(&arbitraryValue))

func maxEventCbIndex() uintptr {
        maxI := arbitraryAddress
        for i := range eventCallbacks {
                if i > maxI {
                        maxI = i
                }
        }
        return maxI
}

func maxLoggerCbIndex() uintptr {
        maxI := arbitraryAddress
        for i := range loggerCallbacks {
                if i > maxI {
                        maxI = i
                }
        }
        return maxI
}

func maxPubkeyCbIndex() uintptr {
        maxI := arbitraryAddress
        for i := range pubkeyCallbacks {
                if i > maxI {
                        maxI = i
                }
        }
        return maxI
}
%}

%typemap(gotype) norddrop_event_cb "func(string)";
%typemap(imtype) norddrop_event_cb "C.norddrop_event_cb";
%typemap(goin) norddrop_event_cb {
        index := maxEventCbIndex() + 1
        cb := C.norddrop_event_cb{
                ctx: unsafe.Pointer(index),
                cb: (C.norddrop_event_fn)(C.call_norddrop_event_cb),
        }
        eventCallbacks[index] = $input
        $result = cb
}
%typemap(in) norddrop_event_cb {
        $1 = $input;
}

%typemap(goout) (struct norddrop *) {
    if $input == SwigcptrNorddrop(0) {
        $result = nil
    }

    $result = $input
}

%insert(go_wrapper) %{
//export call_norddrop_event_cb
func call_norddrop_event_cb(ctx uintptr, str *C.char) {
        if callback, ok := eventCallbacks[ctx]; ok {
                callback(C.GoString(str))
        }
}
%}


%typemap(gotype) norddrop_logger_cb "func(int, string)";
%typemap(imtype) norddrop_logger_cb "C.norddrop_logger_cb";
%typemap(goin) norddrop_logger_cb {
        index := maxLoggerCbIndex() + 1
        cb := C.norddrop_logger_cb{
                ctx: unsafe.Pointer(index),
                cb: (C.norddrop_logger_fn)(C.call_norddrop_logger_cb),
        }
        loggerCallbacks[index] = $input
        $result = cb
}
%typemap(in) norddrop_logger_cb {
        $1 = $input;
}

%insert(go_wrapper) %{
//export call_norddrop_logger_cb
func call_norddrop_logger_cb(ctx uintptr, level C.int, str *C.char) {
        if callback, ok := loggerCallbacks[ctx]; ok {
                callback(int(level), C.GoString(str))
        }
}


%typemap(gotype) norddrop_pubkey_cb "func(byte[], *byte) int";
%typemap(imtype) norddrop_pubkey_cb "C.norddrop_pubkey_cb";
%typemap(goin) norddrop_pubkey_cb {
        index := maxPubkeyCbIndex() + 1
        cb := C.norddrop_pubkey_cb{
                ctx: unsafe.Pointer(index),
                cb: (C.norddrop_pubkey_fn)(C.call_norddrop_pubkey_cb),
        }
        pubkeyCallbacks[index] = $input
        $result = cb
}
%typemap(in) norddrop_pubkey_cb {
        $1 = $input;
}

%insert(go_wrapper) %{
//export call_norddrop_pubkey_cb
func call_norddrop_pubkey_cb(ctx uintptr, ip *C.char, pubkey *C.char) C.int {
        if callback, ok := pubkeyCallbacks[ctx]; ok {
                goip := C.GoBytes(ip, 4)
                gokey := callback(go_ip)

                if gokey != nil {
                        ckey := C.CBytes(gokey)
                        C.memcpy(pubkey, ckey, 32)
                        C.free(ckey)

                        return 0
                }
        }

        return 1
}
%}
#endif
