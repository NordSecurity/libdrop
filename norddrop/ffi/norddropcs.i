

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
%}

%typemap(cscode) norddrop %{
  public delegate void EventDelegate(string message);
  public delegate void LoggerDelegate(NorddropLogLevel level, string message);
%}

%typemap(cstype) norddrop_event_cb "EventDelegate";
%typemap(cstype) norddrop_logger_cb "LoggerDelegate";

%typemap(imtype) norddrop_event_cb "Norddrop.EventDelegate";
%typemap(imtype) norddrop_logger_cb "Norddrop.LoggerDelegate";

%typemap(ctype) norddrop_event_cb "cs_norddrop_event_cb";
%typemap(ctype) norddrop_logger_cb "cs_norddrop_logger_cb";

%typemap(csin) norddrop_event_cb "$csinput";
%typemap(csin) norddrop_logger_cb "$csinput";

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

%extend norddrop {
    %exception norddrop %{
        $action
    %}

    norddrop(norddrop_event_cb events, enum norddrop_log_level level, norddrop_logger_cb logger) {
        norddrop *t = NULL;
        if (NORDDROP_RES_OK != norddrop_new(&t, events, level, logger)) {
            SWIG_CSharpSetPendingException(SWIG_CSharpSystemException,
                                           "Could not initialize library");
            return NULL;
        }
        return t;
    }
}

#endif
