/* ----------------------------------------------------------------------------
 * This file was automatically generated by SWIG (http://www.swig.org).
 * Version 4.0.2
 *
 * This file is not intended to be easily readable and contains a number of
 * coding conventions designed to improve portability and efficiency. Do not make
 * changes to this file unless you know what you are doing--modify the SWIG
 * interface file instead.
 * ----------------------------------------------------------------------------- */


#ifndef SWIGJAVA
#define SWIGJAVA
#endif


/* -----------------------------------------------------------------------------
 *  This section contains generic SWIG labels for method/variable
 *  declarations/attributes, and other compiler dependent labels.
 * ----------------------------------------------------------------------------- */

/* template workaround for compilers that cannot correctly implement the C++ standard */
#ifndef SWIGTEMPLATEDISAMBIGUATOR
# if defined(__SUNPRO_CC) && (__SUNPRO_CC <= 0x560)
#  define SWIGTEMPLATEDISAMBIGUATOR template
# elif defined(__HP_aCC)
/* Needed even with `aCC -AA' when `aCC -V' reports HP ANSI C++ B3910B A.03.55 */
/* If we find a maximum version that requires this, the test would be __HP_aCC <= 35500 for A.03.55 */
#  define SWIGTEMPLATEDISAMBIGUATOR template
# else
#  define SWIGTEMPLATEDISAMBIGUATOR
# endif
#endif

/* inline attribute */
#ifndef SWIGINLINE
# if defined(__cplusplus) || (defined(__GNUC__) && !defined(__STRICT_ANSI__))
#   define SWIGINLINE inline
# else
#   define SWIGINLINE
# endif
#endif

/* attribute recognised by some compilers to avoid 'unused' warnings */
#ifndef SWIGUNUSED
# if defined(__GNUC__)
#   if !(defined(__cplusplus)) || (__GNUC__ > 3 || (__GNUC__ == 3 && __GNUC_MINOR__ >= 4))
#     define SWIGUNUSED __attribute__ ((__unused__))
#   else
#     define SWIGUNUSED
#   endif
# elif defined(__ICC)
#   define SWIGUNUSED __attribute__ ((__unused__))
# else
#   define SWIGUNUSED
# endif
#endif

#ifndef SWIG_MSC_UNSUPPRESS_4505
# if defined(_MSC_VER)
#   pragma warning(disable : 4505) /* unreferenced local function has been removed */
# endif
#endif

#ifndef SWIGUNUSEDPARM
# ifdef __cplusplus
#   define SWIGUNUSEDPARM(p)
# else
#   define SWIGUNUSEDPARM(p) p SWIGUNUSED
# endif
#endif

/* internal SWIG method */
#ifndef SWIGINTERN
# define SWIGINTERN static SWIGUNUSED
#endif

/* internal inline SWIG method */
#ifndef SWIGINTERNINLINE
# define SWIGINTERNINLINE SWIGINTERN SWIGINLINE
#endif

/* exporting methods */
#if defined(__GNUC__)
#  if (__GNUC__ >= 4) || (__GNUC__ == 3 && __GNUC_MINOR__ >= 4)
#    ifndef GCC_HASCLASSVISIBILITY
#      define GCC_HASCLASSVISIBILITY
#    endif
#  endif
#endif

#ifndef SWIGEXPORT
# if defined(_WIN32) || defined(__WIN32__) || defined(__CYGWIN__)
#   if defined(STATIC_LINKED)
#     define SWIGEXPORT
#   else
#     define SWIGEXPORT __declspec(dllexport)
#   endif
# else
#   if defined(__GNUC__) && defined(GCC_HASCLASSVISIBILITY)
#     define SWIGEXPORT __attribute__ ((visibility("default")))
#   else
#     define SWIGEXPORT
#   endif
# endif
#endif

/* calling conventions for Windows */
#ifndef SWIGSTDCALL
# if defined(_WIN32) || defined(__WIN32__) || defined(__CYGWIN__)
#   define SWIGSTDCALL __stdcall
# else
#   define SWIGSTDCALL
# endif
#endif

/* Deal with Microsoft's attempt at deprecating C standard runtime functions */
#if !defined(SWIG_NO_CRT_SECURE_NO_DEPRECATE) && defined(_MSC_VER) && !defined(_CRT_SECURE_NO_DEPRECATE)
# define _CRT_SECURE_NO_DEPRECATE
#endif

/* Deal with Microsoft's attempt at deprecating methods in the standard C++ library */
#if !defined(SWIG_NO_SCL_SECURE_NO_DEPRECATE) && defined(_MSC_VER) && !defined(_SCL_SECURE_NO_DEPRECATE)
# define _SCL_SECURE_NO_DEPRECATE
#endif

/* Deal with Apple's deprecated 'AssertMacros.h' from Carbon-framework */
#if defined(__APPLE__) && !defined(__ASSERT_MACROS_DEFINE_VERSIONS_WITHOUT_UNDERSCORES)
# define __ASSERT_MACROS_DEFINE_VERSIONS_WITHOUT_UNDERSCORES 0
#endif

/* Intel's compiler complains if a variable which was never initialised is
 * cast to void, which is a common idiom which we use to indicate that we
 * are aware a variable isn't used.  So we just silence that warning.
 * See: https://github.com/swig/swig/issues/192 for more discussion.
 */
#ifdef __INTEL_COMPILER
# pragma warning disable 592
#endif


/* Fix for jlong on some versions of gcc on Windows */
#if defined(__GNUC__) && !defined(__INTEL_COMPILER)
  typedef long long __int64;
#endif

/* Fix for jlong on 64-bit x86 Solaris */
#if defined(__x86_64)
# ifdef _LP64
#   undef _LP64
# endif
#endif

#include <jni.h>
#include <stdlib.h>
#include <string.h>


/* Support for throwing Java exceptions */
typedef enum {
  SWIG_JavaOutOfMemoryError = 1,
  SWIG_JavaIOException,
  SWIG_JavaRuntimeException,
  SWIG_JavaIndexOutOfBoundsException,
  SWIG_JavaArithmeticException,
  SWIG_JavaIllegalArgumentException,
  SWIG_JavaNullPointerException,
  SWIG_JavaDirectorPureVirtual,
  SWIG_JavaUnknownError,
  SWIG_JavaIllegalStateException,
} SWIG_JavaExceptionCodes;

typedef struct {
  SWIG_JavaExceptionCodes code;
  const char *java_exception;
} SWIG_JavaExceptions_t;


static void SWIGUNUSED SWIG_JavaThrowException(JNIEnv *jenv, SWIG_JavaExceptionCodes code, const char *msg) {
  jclass excep;
  static const SWIG_JavaExceptions_t java_exceptions[] = {
    { SWIG_JavaOutOfMemoryError, "java/lang/OutOfMemoryError" },
    { SWIG_JavaIOException, "java/io/IOException" },
    { SWIG_JavaRuntimeException, "java/lang/RuntimeException" },
    { SWIG_JavaIndexOutOfBoundsException, "java/lang/IndexOutOfBoundsException" },
    { SWIG_JavaArithmeticException, "java/lang/ArithmeticException" },
    { SWIG_JavaIllegalArgumentException, "java/lang/IllegalArgumentException" },
    { SWIG_JavaNullPointerException, "java/lang/NullPointerException" },
    { SWIG_JavaDirectorPureVirtual, "java/lang/RuntimeException" },
    { SWIG_JavaUnknownError,  "java/lang/UnknownError" },
    { SWIG_JavaIllegalStateException, "java/lang/IllegalStateException" },
    { (SWIG_JavaExceptionCodes)0,  "java/lang/UnknownError" }
  };
  const SWIG_JavaExceptions_t *except_ptr = java_exceptions;

  while (except_ptr->code != code && except_ptr->code)
    except_ptr++;

  (*jenv)->ExceptionClear(jenv);
  excep = (*jenv)->FindClass(jenv, except_ptr->java_exception);
  if (excep)
    (*jenv)->ThrowNew(jenv, excep, msg);
}


/* Contract support */

#define SWIG_contract_assert(nullreturn, expr, msg) if (!(expr)) {SWIG_JavaThrowException(jenv, SWIG_JavaIllegalArgumentException, msg); return nullreturn; } else


#include "../../norddrop.h"


#include "jni_helper.h"
#define PKG "com/nordsec/norddrop/"
static JavaVM *jvm = NULL;


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



DECLARE_CACHED_CLASS(iNordDropPubkeyCb, PKG "INordDropPubkeyCb");
DECLARE_CACHED_METHOD_ID(iNordDropPubkeyCb, iNordDropPubkeyCbPubkeyHandleID, "pubkeyHandle", "(Ljava/lang/String;[B)I");

static int norddrop_jni_call_pubkey_cb(void *ctx, const char* ip, char *pubkey) {
    if (!jvm) {
        return 1;
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
            return 1;
        }
        attached = 1;
    } else if (JNI_OK != res) {
        return 1;
    }

    jmethodID handle = GET_CACHED_METHOD_ID(env, iNordDropPubkeyCbPubkeyHandleID);
    RETURN_VAL_AND_THROW_IF_NULL(env, handle, "pubkeyHandle not found.", 1);

    jstring jip = NULL;
    if (ip != NULL) {
        jip = (*env)->NewStringUTF(env, ip);
        RETURN_VAL_AND_THROW_IF_NULL(env, jip, "IP string is null.", 1);
    }

    jstring jpubkey = (*env)->NewByteArray(env, 32);
    RETURN_VAL_AND_THROW_IF_NULL(env, jpubkey, "Cannot crate pubkey array.", 1);

    int cb_res = (*env)->CallIntMethod(env, (jobject)ctx, handle, jip, jpubkey);
    (*env)->GetByteArrayRegion(env, jpubkey, 0, 32, (jbyte*)pubkey);

    if (jip != NULL) {
        (*env)->DeleteLocalRef(env, jip);
    }
    (*env)->DeleteLocalRef(env, jpubkey);

    if (attached) {
        (*jvm)->DetachCurrentThread(jvm);
    }

    return cb_res;
}

SWIGINTERN struct norddrop *new_norddrop(norddrop_event_cb events,enum norddrop_log_level level,norddrop_logger_cb logger,norddrop_pubkey_cb pubkey_cb,char const *privkey){

        JNIEnv *env = NULL;
        norddrop *t = NULL;
        enum norddrop_result result;

        if ((*jvm)->GetEnv(jvm, (void**)&env, JNI_VERSION_1_6)) {
            SWIG_JavaThrowException(env, SWIG_JavaRuntimeException, "Thread not attached to JVM");
            return NULL;
        }

        result = norddrop_new(&t, events, level, logger, pubkey_cb, privkey);
        if (result != NORDDROP_RES_OK) {
            SWIG_JavaThrowException(env, SWIG_JavaIllegalArgumentException, "Could not initialize library");
            return NULL;
        }

        // Find necessary methods and classes so they are cached
        GET_CACHED_METHOD_ID(env, iNordDropLoggerCbloggerHandleID);
        GET_CACHED_METHOD_ID(env, iNordDropEventCbeventHandleID);
        GET_CACHED_METHOD_ID(env, iNordDropPubkeyCbPubkeyHandleID);
        GET_CACHED_CLASS(env, norddropLogLevel);

        return t;
    }
SWIGINTERN void delete_norddrop(struct norddrop *self){
        norddrop_destroy(self);
    }

#ifdef __cplusplus
extern "C" {
#endif

SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1LOG_1CRITICAL_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_log_level result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_log_level)NORDDROP_LOG_CRITICAL;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1LOG_1ERROR_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_log_level result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_log_level)NORDDROP_LOG_ERROR;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1LOG_1WARNING_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_log_level result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_log_level)NORDDROP_LOG_WARNING;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1LOG_1INFO_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_log_level result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_log_level)NORDDROP_LOG_INFO;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1LOG_1DEBUG_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_log_level result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_log_level)NORDDROP_LOG_DEBUG;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1LOG_1TRACE_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_log_level result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_log_level)NORDDROP_LOG_TRACE;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1OK_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_OK;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1ERROR_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_ERROR;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1INVALID_1STRING_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_INVALID_STRING;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1BAD_1INPUT_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_BAD_INPUT;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1JSON_1PARSE_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_JSON_PARSE;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1TRANSFER_1CREATE_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_TRANSFER_CREATE;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1NOT_1STARTED_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_NOT_STARTED;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1ADDR_1IN_1USE_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_ADDR_IN_USE;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1INSTANCE_1START_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_INSTANCE_START;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1INSTANCE_1STOP_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_INSTANCE_STOP;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1INVALID_1PRIVKEY_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_INVALID_PRIVKEY;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NORDDROP_1RES_1DB_1ERROR_1get(JNIEnv *jenv, jclass jcls) {
  jint jresult = 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  result = (enum norddrop_result)NORDDROP_RES_DB_ERROR;
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jlong JNICALL Java_com_nordsec_norddrop_libnorddropJNI_new_1NordDrop(JNIEnv *jenv, jclass jcls, jobject jarg1, jint jarg2, jobject jarg3, jobject jarg4, jbyteArray jarg5) {
  jlong jresult = 0 ;
  norddrop_event_cb arg1 ;
  enum norddrop_log_level arg2 ;
  norddrop_logger_cb arg3 ;
  norddrop_pubkey_cb arg4 ;
  char *arg5 = (char *) 0 ;
  struct norddrop *result = 0 ;
  
  (void)jenv;
  (void)jcls;
  {
    if (!jvm) {
      (*jenv)->GetJavaVM(jenv, &jvm);
    }
    
    norddrop_event_cb cb = {
      .ctx = (*jenv)->NewGlobalRef(jenv, jarg1),
      .cb = norddrop_jni_call_event_cb,
    };
    
    arg1 = cb;
  }
  arg2 = (enum norddrop_log_level)jarg2; 
  {
    if (!jvm) {
      (*jenv)->GetJavaVM(jenv, &jvm);
    }
    norddrop_logger_cb cb = {
      .ctx = (*jenv)->NewGlobalRef(jenv, jarg3),
      .cb = norddrop_jni_call_logger_cb,
    };
    arg3 = cb;
  }
  {
    if (!jvm) {
      (*jenv)->GetJavaVM(jenv, &jvm);
    }
    norddrop_pubkey_cb cb = {
      .ctx = (*jenv)->NewGlobalRef(jenv, jarg4),
      .cb = norddrop_jni_call_pubkey_cb,
    };
    arg4 = cb;
  }
  {
    arg5 = (char *) (*jenv)->GetByteArrayElements(jenv, jarg5, 0); 
  }
  result = (struct norddrop *)new_norddrop(arg1,arg2,arg3,arg4,(char const *)arg5);
  *(struct norddrop **)&jresult = result; 
  {
    (*jenv)->ReleaseByteArrayElements(jenv, jarg5, (jbyte *) arg5, 0); 
  }
  
  return jresult;
}


SWIGEXPORT void JNICALL Java_com_nordsec_norddrop_libnorddropJNI_delete_1NordDrop(JNIEnv *jenv, jclass jcls, jlong jarg1) {
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  
  (void)jenv;
  (void)jcls;
  arg1 = *(struct norddrop **)&jarg1; 
  delete_norddrop(arg1);
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1start(JNIEnv *jenv, jclass jcls, jlong jarg1, jobject jarg1_, jstring jarg2, jstring jarg3) {
  jint jresult = 0 ;
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  (void)jarg1_;
  arg1 = *(struct norddrop **)&jarg1; 
  arg2 = 0;
  if (jarg2) {
    arg2 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg2, 0);
    if (!arg2) return 0;
  }
  arg3 = 0;
  if (jarg3) {
    arg3 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg3, 0);
    if (!arg3) return 0;
  }
  result = (enum norddrop_result)norddrop_start(arg1,(char const *)arg2,(char const *)arg3);
  jresult = (jint)result; 
  if (arg2) (*jenv)->ReleaseStringUTFChars(jenv, jarg2, (const char *)arg2);
  if (arg3) (*jenv)->ReleaseStringUTFChars(jenv, jarg3, (const char *)arg3);
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1stop(JNIEnv *jenv, jclass jcls, jlong jarg1, jobject jarg1_) {
  jint jresult = 0 ;
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  (void)jarg1_;
  arg1 = *(struct norddrop **)&jarg1; 
  result = (enum norddrop_result)norddrop_stop(arg1);
  jresult = (jint)result; 
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1cancelTransfer(JNIEnv *jenv, jclass jcls, jlong jarg1, jobject jarg1_, jstring jarg2) {
  jint jresult = 0 ;
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  (void)jarg1_;
  arg1 = *(struct norddrop **)&jarg1; 
  arg2 = 0;
  if (jarg2) {
    arg2 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg2, 0);
    if (!arg2) return 0;
  }
  result = (enum norddrop_result)norddrop_cancel_transfer(arg1,(char const *)arg2);
  jresult = (jint)result; 
  if (arg2) (*jenv)->ReleaseStringUTFChars(jenv, jarg2, (const char *)arg2);
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1cancelFile(JNIEnv *jenv, jclass jcls, jlong jarg1, jobject jarg1_, jstring jarg2, jstring jarg3) {
  jint jresult = 0 ;
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  (void)jarg1_;
  arg1 = *(struct norddrop **)&jarg1; 
  arg2 = 0;
  if (jarg2) {
    arg2 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg2, 0);
    if (!arg2) return 0;
  }
  arg3 = 0;
  if (jarg3) {
    arg3 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg3, 0);
    if (!arg3) return 0;
  }
  result = (enum norddrop_result)norddrop_cancel_file(arg1,(char const *)arg2,(char const *)arg3);
  jresult = (jint)result; 
  if (arg2) (*jenv)->ReleaseStringUTFChars(jenv, jarg2, (const char *)arg2);
  if (arg3) (*jenv)->ReleaseStringUTFChars(jenv, jarg3, (const char *)arg3);
  return jresult;
}


SWIGEXPORT jint JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1download(JNIEnv *jenv, jclass jcls, jlong jarg1, jobject jarg1_, jstring jarg2, jstring jarg3, jstring jarg4) {
  jint jresult = 0 ;
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  char *arg4 = (char *) 0 ;
  enum norddrop_result result;
  
  (void)jenv;
  (void)jcls;
  (void)jarg1_;
  arg1 = *(struct norddrop **)&jarg1; 
  arg2 = 0;
  if (jarg2) {
    arg2 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg2, 0);
    if (!arg2) return 0;
  }
  arg3 = 0;
  if (jarg3) {
    arg3 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg3, 0);
    if (!arg3) return 0;
  }
  arg4 = 0;
  if (jarg4) {
    arg4 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg4, 0);
    if (!arg4) return 0;
  }
  result = (enum norddrop_result)norddrop_download(arg1,(char const *)arg2,(char const *)arg3,(char const *)arg4);
  jresult = (jint)result; 
  if (arg2) (*jenv)->ReleaseStringUTFChars(jenv, jarg2, (const char *)arg2);
  if (arg3) (*jenv)->ReleaseStringUTFChars(jenv, jarg3, (const char *)arg3);
  if (arg4) (*jenv)->ReleaseStringUTFChars(jenv, jarg4, (const char *)arg4);
  return jresult;
}


SWIGEXPORT jstring JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1newTransfer(JNIEnv *jenv, jclass jcls, jlong jarg1, jobject jarg1_, jstring jarg2, jstring jarg3) {
  jstring jresult = 0 ;
  struct norddrop *arg1 = (struct norddrop *) 0 ;
  char *arg2 = (char *) 0 ;
  char *arg3 = (char *) 0 ;
  char *result = 0 ;
  
  (void)jenv;
  (void)jcls;
  (void)jarg1_;
  arg1 = *(struct norddrop **)&jarg1; 
  arg2 = 0;
  if (jarg2) {
    arg2 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg2, 0);
    if (!arg2) return 0;
  }
  arg3 = 0;
  if (jarg3) {
    arg3 = (char *)(*jenv)->GetStringUTFChars(jenv, jarg3, 0);
    if (!arg3) return 0;
  }
  result = (char *)norddrop_new_transfer(arg1,(char const *)arg2,(char const *)arg3);
  if (result) jresult = (*jenv)->NewStringUTF(jenv, (const char *)result);
  if (arg2) (*jenv)->ReleaseStringUTFChars(jenv, jarg2, (const char *)arg2);
  if (arg3) (*jenv)->ReleaseStringUTFChars(jenv, jarg3, (const char *)arg3);
  free(result);
  return jresult;
}


SWIGEXPORT jstring JNICALL Java_com_nordsec_norddrop_libnorddropJNI_NordDrop_1version(JNIEnv *jenv, jclass jcls) {
  jstring jresult = 0 ;
  char *result = 0 ;
  
  (void)jenv;
  (void)jcls;
  result = (char *)norddrop_version();
  if (result) jresult = (*jenv)->NewStringUTF(jenv, (const char *)result);
  return jresult;
}


#ifdef __cplusplus
}
#endif

