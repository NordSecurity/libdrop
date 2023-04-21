

#if SWIGCSHARP

%rename("%(lowercamelcase)s") "";
%rename("%(camelcase)s", %$isfunction) "";
%rename("%(camelcase)s", %$isclass) "";
%rename("Norddrop") "norddrop";

%{
typedef void(*cs_norddrop_event_cb)(const char *);
void call_norddrop_event_cb(void *ctx, const char *msg) {
  cs_norddrop_event_cb cb = ctx;
  cb(msg);
}

typedef void(*cs_norddrop_logger_cb)(int, const char *);
void call_norddrop_logger_cb(void *ctx, int l, const char *msg) {
  cs_norddrop_logger_cb cb = ctx;
  cb(l, msg);
}

typedef int(*cs_norddrop_pubkey_cb)(const unsigned char*, unsigned char*);
int call_norddrop_pubkey_cb(void *ctx, const char* ip, char* privkey) {
  cs_norddrop_pubkey_cb cb = ctx;
  return cb(ip, privkey);
}
%}

%typemap(cscode) norddrop %{
  public delegate void EventDelegate(string message);
  public delegate void LoggerDelegate(NorddropLogLevel level, string message);
  public delegate int PubkeyDelegate(string ip, out byte[] pubkey);
%}

%typemap(cstype) norddrop_event_cb "EventDelegate";
%typemap(cstype) norddrop_logger_cb "LoggerDelegate";
%typemap(cstype) norddrop_pubkey_cb "PubkeyDelegate";

%typemap(imtype) norddrop_event_cb "Norddrop.EventDelegate";
%typemap(imtype) norddrop_logger_cb "Norddrop.LoggerDelegate";
%typemap(imtype) norddrop_pubkey_cb "Norddrop.PubkeyDelegate";

%typemap(ctype) norddrop_event_cb "cs_norddrop_event_cb";
%typemap(ctype) norddrop_logger_cb "cs_norddrop_logger_cb";
%typemap(ctype) norddrop_pubkey_cb "cs_norddrop_pubkey_cb";

%typemap(csin) norddrop_event_cb "$csinput";
%typemap(csin) norddrop_logger_cb "$csinput";
%typemap(csin) norddrop_pubkey_cb "$csinput";

%typemap(in) norddrop_event_cb %{ 
  $1 = (struct norddrop_event_cb) {
    .ctx = $input,
    .cb = call_norddrop_event_cb,
  };
%}


%typemap(in) norddrop_logger_cb %{
  $1 = (struct norddrop_logger_cb) {
    .ctx = $input,
    .cb = call_norddrop_logger_cb,
  };
%}

%typemap(in) norddrop_pubkey_cb %{
  $1 = (struct norddrop_pubkey_cb) {
    .ctx = $input,
    .cb = call_norddrop_pubkey_cb,
  };
%}

%extend norddrop {
    %exception norddrop %{
        $action
    %}

    norddrop(norddrop_event_cb events,
        enum norddrop_log_level level,
        norddrop_logger_cb logger,
        norddrop_pubkey_cb pubkey_cb,
        const char* privkey) {

        norddrop *t = NULL;
        if (NORDDROP_RES_OK != norddrop_new(&t, events, level, logger, pubkey_cb, privkey)) {
            SWIG_CSharpSetPendingException(SWIG_CSharpSystemException,
                                           "Could not initialize library");
            return NULL;
        }
        return t;
    }
}

#endif
