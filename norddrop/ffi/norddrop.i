%module libnorddrop

%{
#include "../../norddrop.h"
%}

%rename("%(camelcase)s") "";
%rename("$ignore", fullname=1) __norddrop_force_export;
%rename("$ignore", fullname=1) __norddrop_generate_panic;

#if SWIGJAVA || SWIGCSHARP
%rename("$ignore", regexmatch$name=".*_cb") "";
%rename("$ignore", regexmatch$name=".*_fn") "";
#endif

#if SWIGJAVA
%rename("%(strip:[norddrop_])s", %$isenumitem) "";
#endif

#if SWIGCSHARP
/* XXX: This will break in some future release: https://github.com/swig/swig/issues/1806
 *
 * TL;DR: popen(3) invokes sh(1) to run the command, which is exactly what SWIG
 *        uses. However, Debian (and, in particular, derivatives) use dash as
 *        /bin/sh, where bash-isms (<<< here strings) don’t work:
 *
 *            sh: 1: Syntax error: redirection unexpected
 *
 * There is currently no good replacement other than writing unwieldy regular
 * expressions.
 */
%rename("%(command:cs_rename<<<)s", %$isenumitem) "";
#endif

%include "norddrop_types.h";

%include "norddropgo.i"
%include "norddropjava.i"
%include "norddropcs.i"

struct norddrop {};

%extend norddrop {

    norddrop(norddrop_event_cb events,
        enum norddrop_log_level level,
        norddrop_logger_cb logger,
        norddrop_pubkey_cb pubkey_cb,
        const char* privkey) {

        norddrop *t = NULL;
        if (NORDDROP_RES_OK != norddrop_new(&t, events, level, logger, pubkey_cb, privkey)) {
            return NULL;
        }
        return t;
    }

    ~norddrop() {
        norddrop_destroy($self);
    }

    // Start drop server. Listens for incoming connections. Allows files and responses to be received
    enum norddrop_result start(const char *listen_addr, const char* config_json);

    // Stop drop server. Will not be reachable for peers
    enum norddrop_result stop();
        
    // Cancel the whole the transfer request
    enum norddrop_result cancel_transfer(const char* txid);

    // Cancel a single file in a request
    enum norddrop_result cancel_file(const char* txid, const char* fid);

    // Download a file to a destination path
    enum norddrop_result download(const char* txid, const char* fid, const char* dst_path);

    %newobject new_transfer;
    // Create a new transfer for the given descriptor(s). Returns transfer id(xfid)
    char* new_transfer(const char* peer, const char* descriptors);

    // Purge transfers with the given id(s) from the database, accepts a JSON
    // array of strings
    enum norddrop_result purge_transfers(const char *txids);

    // Purge all transfers that are older than the given timestamp from the
    // database. Accepts a UNIX timestamp in seconds
    enum norddrop_result purge_transfers_until(int64_t until_timestamp);

    %newobject get_transfers_since;
    // Get all transfers since the given timestamp from the database.
    // Accepts a UNIX timestamp in seconds
    char *get_transfers_since(int64_t since_timestamp);

    // Returns current version of the library
    static char* version();
};

