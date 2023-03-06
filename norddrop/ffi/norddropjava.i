#if SWIGJAVA

%rename("%(lowercamelcase)s") "";
%rename("NordDrop") "norddrop";

%{
#include "jni_helper.h"
#define PKG "com/nordsec/norddrop/"
static JavaVM *jvm = NULL;
%}

///////////////////////////////////////////////////////////////
// Wrap norddrop_event_cb into java interface

// INordDropEventCb.java is manualy written.
%typemap(jstype) norddrop_event_cb "INordDropEventCb"
%typemap(jtype) norddrop_event_cb "INordDropEventCb"
%typemap(jni) norddrop_event_cb "jobject"
%typemap(javain) norddrop_event_cb "$javainput"

%extend norddrop {
    norddrop(norddrop_event_cb events, enum norddrop_log_level level, norddrop_logger_cb logger) {
        JNIEnv *env = NULL;
        norddrop *t = NULL;
        enum norddrop_result result;

        if ((*jvm)->GetEnv(jvm, (void**)&env, JNI_VERSION_1_6)) {
            SWIG_JavaThrowException(env, SWIG_JavaRuntimeException, "Thread not attached to JVM");
            return NULL;
        }

        result = norddrop_new(&t, events, level, logger);
        if (result != NORDDROP_RES_OK) {
            SWIG_JavaThrowException(env, SWIG_JavaIllegalArgumentException, "Could not initialize library");
            return NULL;
        }

        // Find necessary methods and classes so they are cached
        GET_CACHED_METHOD_ID(env, iNordDropLoggerCbloggerHandleID);
        GET_CACHED_METHOD_ID(env, iNordDropEventCbeventHandleID);
        GET_CACHED_CLASS(env, norddropLogLevel);

        return t;
    }
}

%{
DECLARE_CACHED_CLASS(iNordDropEventCb, PKG "INordDropEventCb");
DECLARE_CACHED_METHOD_ID(iNordDropEventCb, iNordDropEventCbeventHandleID, "eventHandle", "(Ljava/lang/String;)V");

static void norddrop_jni_call_event_cb(void *ctx, const char *str) {
    if (!jvm) {
        return;
    }
    JNIEnv *env = NULL;
    
    jint res = (*jvm)->GetEnv(jvm, (void**)&env, JNI_VERSION_1_6);
    int attached = 0;
    if (JNI_EDETACHED == res) {
        JavaVMAttachArgs args = {
            .version = JNI_VERSION_1_6,
            .name = NULL,
            .group = NULL,
        };

        if ((*jvm)->AttachCurrentThread(jvm, &env, (void*)&args)) {
            return;
        }
        attached = 1;
    } else if (JNI_OK != res) {
        return;
    }

    jmethodID handle = GET_CACHED_METHOD_ID(env, iNordDropEventCbeventHandleID);
    RETURN_AND_THROW_IF_NULL(env, handle, "eventHandle method not found.");

    jstring jstr = (*env)->NewStringUTF(env, str);
    RETURN_AND_THROW_IF_NULL(env, jstr, "Event string is null.");

    (*env)->CallVoidMethod(env, (jobject)ctx, handle, jstr);
    (*env)->DeleteLocalRef(env, jstr);
    if (attached) {
        (*jvm)->DetachCurrentThread(jvm);
    }
}
%}

// TODO: Add destructor for callback.
%typemap(in) norddrop_event_cb {
    if (!jvm) {
        (*jenv)->GetJavaVM(jenv, &jvm);
    }

    norddrop_event_cb cb = {
        .ctx = (*jenv)->NewGlobalRef(jenv, $input),
        .cb = norddrop_jni_call_event_cb,
    };

    $1 = cb;
}

///////////////////////////////////////////////////////////////
// Wrap norddrop_logger_cb into java interface

// INordDropLoggerCb.java is manualy written.
%typemap(jstype) norddrop_logger_cb "INordDropLoggerCb"
%typemap(jtype) norddrop_logger_cb "INordDropLoggerCb"
%typemap(jni) norddrop_logger_cb "jobject"
%typemap(javain) norddrop_logger_cb "$javainput"

%{

DECLARE_CACHED_CLASS(norddropLogLevel, PKG "NorddropLogLevel");
DECLARE_CACHED_STATIC_FIELD_ID(norddropLogLevel, jLogLevelCritical, "NORDDROP_LOG_CRITICAL", "L" PKG "NorddropLogLevel;");
DECLARE_CACHED_STATIC_FIELD_ID(norddropLogLevel, jLogLevelError,    "NORDDROP_LOG_ERROR", "L" PKG "NorddropLogLevel;");
DECLARE_CACHED_STATIC_FIELD_ID(norddropLogLevel, jLogLevelWarning,  "NORDDROP_LOG_WARNING", "L" PKG "NorddropLogLevel;");
DECLARE_CACHED_STATIC_FIELD_ID(norddropLogLevel, jLogLevelInfo,     "NORDDROP_LOG_INFO", "L" PKG "NorddropLogLevel;");
DECLARE_CACHED_STATIC_FIELD_ID(norddropLogLevel, jLogLevelDebug,    "NORDDROP_LOG_DEBUG", "L" PKG "NorddropLogLevel;");
DECLARE_CACHED_STATIC_FIELD_ID(norddropLogLevel, jLogLevelTrace,    "NORDDROP_LOG_TRACE", "L" PKG "NorddropLogLevel;");

DECLARE_CACHED_CLASS(iNordDropLoggerCb, PKG "INordDropLoggerCb");
DECLARE_CACHED_METHOD_ID(iNordDropLoggerCb, iNordDropLoggerCbloggerHandleID, "loggerHandle", "(L" PKG "NorddropLogLevel;Ljava/lang/String;)V");

static void norddrop_jni_call_logger_cb(void *ctx, enum norddrop_log_level level, const char *str) {
    if (!jvm) {
        return;
    }

    JNIEnv *env = NULL;

    jint res = (*jvm)->GetEnv(jvm, (void**)&env, JNI_VERSION_1_6);
    int attached = 0;
    if (JNI_EDETACHED == res) {
        JavaVMAttachArgs args = {
            .version = JNI_VERSION_1_6,
            .name = NULL,
            .group = NULL,
        };

        if ((*jvm)->AttachCurrentThread(jvm, &env, (void*)&args)) {
            return;
        }
        attached = 1;
    } else if (JNI_OK != res) {
        return;
    }

    jmethodID handle = GET_CACHED_METHOD_ID(env, iNordDropLoggerCbloggerHandleID);
    RETURN_AND_THROW_IF_NULL(env, handle, "loggerHandle not found.");

    jstring jstr = (*env)->NewStringUTF(env, str);
    RETURN_AND_THROW_IF_NULL(env, jstr, "Cannot crate log string.");

    jfieldID lfid = NULL;
    jclass jlevelClass = GET_CACHED_CLASS(env, norddropLogLevel);
    RETURN_AND_THROW_IF_NULL(env, jlevelClass, "could not find NordDropLogLevel class .");
    jobject jlevel = NULL;
    #define MAP(level, field) \
        case level:\
            lfid = GET_CACHED_STATIC_FIELD_ID(env, field);\
            RETURN_AND_THROW_IF_NULL(env, lfid, #level " level class not found.")\
            jlevel = (*env)->GetStaticObjectField(env, jlevelClass, lfid);\
            RETURN_AND_THROW_IF_NULL(env, jlevel, #level " level class not found.")\
            break;
    switch (level) {
        MAP(NORDDROP_LOG_CRITICAL, jLogLevelCritical)
        MAP(NORDDROP_LOG_ERROR, jLogLevelError)
        MAP(NORDDROP_LOG_WARNING, jLogLevelWarning)
        MAP(NORDDROP_LOG_INFO, jLogLevelInfo)
        MAP(NORDDROP_LOG_DEBUG, jLogLevelDebug)
        MAP(NORDDROP_LOG_TRACE, jLogLevelTrace)
    }
    #undef MAP

    (*env)->CallVoidMethod(env, (jobject)ctx, handle, jlevel, jstr);
    (*env)->DeleteLocalRef(env, jlevel);
    (*env)->DeleteLocalRef(env, jstr);
    if (attached) {
        (*jvm)->DetachCurrentThread(jvm);
    }
}
%}

%typemap(in) norddrop_logger_cb {
    if (!jvm) {
        (*jenv)->GetJavaVM(jenv, &jvm);
    }
    norddrop_logger_cb cb = {
        .ctx = (*jenv)->NewGlobalRef(jenv, $input),
        .cb = norddrop_jni_call_logger_cb,
    };
    $1 = cb;
}

#endif

